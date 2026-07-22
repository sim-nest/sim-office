//! SQLite persistence for local Gantt plans.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use time::Date;

use crate::critical::{ScheduleError, validate_plan};
use crate::model::{GanttPlan, LinkKind, Task, TaskLink};

/// SQLite-backed local store for schedule plans.
pub struct GanttStore {
    conn: Connection,
}

impl GanttStore {
    /// Opens or creates a local Gantt store at `path`.
    pub fn create(path: &Path) -> Result<Self, ScheduleError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(include_str!("schema.sql"))?;
        Ok(Self { conn })
    }

    /// Saves a whole plan snapshot, replacing any existing snapshot for its id.
    pub fn save_plan(&mut self, plan: &GanttPlan) -> Result<(), ScheduleError> {
        validate_plan(plan)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO gantt_plans (id) VALUES (?1)
             ON CONFLICT(id) DO UPDATE SET id = excluded.id",
            params![plan.id],
        )?;
        tx.execute(
            "DELETE FROM gantt_links WHERE plan_id = ?1",
            params![plan.id],
        )?;
        tx.execute(
            "DELETE FROM gantt_tasks WHERE plan_id = ?1",
            params![plan.id],
        )?;

        for (position, task) in plan.tasks.iter().enumerate() {
            tx.execute(
                "INSERT INTO gantt_tasks
                 (plan_id, id, name, start_julian, finish_julian, percent_complete, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    plan.id,
                    task.id,
                    task.name,
                    task.start.to_julian_day(),
                    task.finish.to_julian_day(),
                    i64::from(task.percent_complete),
                    sqlite_position(position)?,
                ],
            )?;
        }

        for (position, link) in plan.links.iter().enumerate() {
            tx.execute(
                "INSERT INTO gantt_links
                 (plan_id, predecessor, successor, kind, lag_days, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    plan.id,
                    link.predecessor,
                    link.successor,
                    link.kind.as_str(),
                    i64::from(link.lag_days),
                    sqlite_position(position)?,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Loads a plan snapshot by id.
    pub fn load_plan(&self, id: &str) -> Result<Option<GanttPlan>, ScheduleError> {
        let plan_id = self
            .conn
            .query_row(
                "SELECT id FROM gantt_plans WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(plan_id) = plan_id else {
            return Ok(None);
        };

        let tasks = self.load_tasks(&plan_id)?;
        let links = self.load_links(&plan_id)?;
        Ok(Some(GanttPlan::new(plan_id, tasks, links)))
    }

    fn load_tasks(&self, plan_id: &str) -> Result<Vec<Task>, ScheduleError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, start_julian, finish_julian, percent_complete
             FROM gantt_tasks WHERE plan_id = ?1 ORDER BY position ASC",
        )?;
        let rows = stmt.query_map(params![plan_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;

        let mut tasks = Vec::new();
        for row in rows {
            let (id, name, start, finish, percent) = row?;
            tasks.push(Task::new(
                id,
                name,
                date_from_julian(start, "start_julian")?,
                date_from_julian(finish, "finish_julian")?,
                percent_from_sql(percent)?,
            ));
        }
        Ok(tasks)
    }

    fn load_links(&self, plan_id: &str) -> Result<Vec<TaskLink>, ScheduleError> {
        let mut stmt = self.conn.prepare(
            "SELECT predecessor, successor, kind, lag_days
             FROM gantt_links WHERE plan_id = ?1 ORDER BY position ASC",
        )?;
        let rows = stmt.query_map(params![plan_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;

        let mut links = Vec::new();
        for row in rows {
            let (predecessor, successor, kind, lag_days) = row?;
            let kind = LinkKind::from_token(&kind)
                .ok_or_else(|| ScheduleError::Store(format!("unknown gantt link kind {kind}")))?;
            links.push(TaskLink::new(
                predecessor,
                successor,
                kind,
                i32_from_sql(lag_days, "lag_days")?,
            ));
        }
        Ok(links)
    }
}

impl From<rusqlite::Error> for ScheduleError {
    fn from(error: rusqlite::Error) -> Self {
        ScheduleError::Store(error.to_string())
    }
}

fn sqlite_position(position: usize) -> Result<i64, ScheduleError> {
    i64::try_from(position).map_err(|_| {
        ScheduleError::Store(format!("position {position} does not fit sqlite INTEGER"))
    })
}

fn date_from_julian(value: i64, field: &'static str) -> Result<Date, ScheduleError> {
    let day = i32_from_sql(value, field)?;
    Date::from_julian_day(day)
        .map_err(|error| ScheduleError::Store(format!("invalid {field}: {error}")))
}

fn percent_from_sql(value: i64) -> Result<u8, ScheduleError> {
    let percent = u8::try_from(value)
        .map_err(|_| ScheduleError::Store(format!("percent_complete {value} is out of range")))?;
    if percent > 100 {
        return Err(ScheduleError::Store(format!(
            "percent_complete {value} is out of range"
        )));
    }
    Ok(percent)
}

fn i32_from_sql(value: i64, field: &'static str) -> Result<i32, ScheduleError> {
    i32::try_from(value)
        .map_err(|_| ScheduleError::Store(format!("{field} {value} is out of range")))
}

#[cfg(test)]
mod tests {
    use time::{Date, Month};

    use super::*;

    fn date(day: u8) -> Date {
        Date::from_calendar_date(2026, Month::July, day).unwrap()
    }

    fn plan() -> GanttPlan {
        GanttPlan::new(
            "plan-1",
            vec![
                Task::new("design", "Design", date(1), date(3), 50),
                Task::new("build", "Build", date(3), date(5), 0),
            ],
            vec![TaskLink::new("design", "build", LinkKind::FinishStart, 0)],
        )
    }

    #[test]
    fn two_linked_tasks_save_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = GanttStore::create(&dir.path().join("gantt.sqlite")).unwrap();
        let plan = plan();

        store.save_plan(&plan).unwrap();
        let loaded = store.load_plan("plan-1").unwrap().unwrap();

        assert_eq!(loaded, plan);
    }
}
