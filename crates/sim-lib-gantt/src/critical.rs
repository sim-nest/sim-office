//! Critical path analysis for local Gantt plans.

use std::collections::{BTreeMap, VecDeque};

use sim_kernel::Cx;
use sim_lib_discrete_graph::{Directedness, Graph, GraphError, strongly_connected_components};
use thiserror::Error;

use crate::model::{GanttPlan, LinkKind};

/// Errors returned by schedule validation, analysis, and persistence.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ScheduleError {
    /// A plan or task field is invalid.
    #[error("invalid schedule field {field}: {message}")]
    InvalidField {
        /// Field or record identifier.
        field: String,
        /// Human-facing reason.
        message: String,
    },
    /// A task id appears more than once.
    #[error("duplicate task id {0}")]
    DuplicateTask(String),
    /// A dependency references a task that is not in the plan.
    #[error("missing task id {0}")]
    MissingTask(String),
    /// Dependency analysis found a directed cycle.
    #[error("cyclic task dependency: {}", .0.join(" -> "))]
    Cycle(Vec<String>),
    /// The shared discrete graph layer rejected the schedule graph.
    #[error("schedule graph error: {0}")]
    Graph(String),
    /// The local backend could not read or write the plan.
    #[error("gantt store error: {0}")]
    Store(String),
}

/// Returns task ids that have zero slack on the local critical path.
///
/// The kernel context is accepted for parity with other SIM analysis entry
/// points; this local pass does not mutate it.
pub fn critical_tasks(_cx: &mut Cx, plan: &GanttPlan) -> Result<Vec<String>, ScheduleError> {
    let durations = validate_plan(plan)?;
    let analysis = ScheduleGraph::new(plan, durations)?;
    analysis.ensure_acyclic()?;
    let order = analysis.topological_order()?;
    let earliest = analysis.earliest_starts(&order)?;
    let latest = analysis.latest_starts(&order, &earliest)?;

    Ok(plan
        .tasks
        .iter()
        .enumerate()
        .filter(|(index, _task)| earliest[*index] == latest[*index])
        .map(|(_index, task)| task.id.clone())
        .collect())
}

pub(crate) fn validate_plan(plan: &GanttPlan) -> Result<Vec<i64>, ScheduleError> {
    if plan.id.trim().is_empty() {
        return Err(ScheduleError::InvalidField {
            field: "plan.id".to_owned(),
            message: "must not be empty".to_owned(),
        });
    }

    let mut seen = BTreeMap::new();
    let mut durations = Vec::with_capacity(plan.tasks.len());
    for (index, task) in plan.tasks.iter().enumerate() {
        if task.id.trim().is_empty() {
            return Err(ScheduleError::InvalidField {
                field: format!("tasks[{index}].id"),
                message: "must not be empty".to_owned(),
            });
        }
        if task.name.trim().is_empty() {
            return Err(ScheduleError::InvalidField {
                field: format!("task {} name", task.id),
                message: "must not be empty".to_owned(),
            });
        }
        if task.percent_complete > 100 {
            return Err(ScheduleError::InvalidField {
                field: format!("task {} percent_complete", task.id),
                message: "must be in 0..=100".to_owned(),
            });
        }
        let duration = task.duration_days();
        if duration < 0 {
            return Err(ScheduleError::InvalidField {
                field: format!("task {} finish", task.id),
                message: "must not be before start".to_owned(),
            });
        }
        if seen.insert(task.id.as_str(), index).is_some() {
            return Err(ScheduleError::DuplicateTask(task.id.clone()));
        }
        durations.push(i64::from(duration));
    }

    for link in &plan.links {
        if !seen.contains_key(link.predecessor.as_str()) {
            return Err(ScheduleError::MissingTask(link.predecessor.clone()));
        }
        if !seen.contains_key(link.successor.as_str()) {
            return Err(ScheduleError::MissingTask(link.successor.clone()));
        }
    }

    Ok(durations)
}

struct ScheduleGraph<'a> {
    plan: &'a GanttPlan,
    durations: Vec<i64>,
    graph: Graph<String, i64>,
}

impl<'a> ScheduleGraph<'a> {
    fn new(plan: &'a GanttPlan, durations: Vec<i64>) -> Result<Self, ScheduleError> {
        let index = task_index(plan);
        let mut graph = Graph::with_nodes(
            plan.tasks.iter().map(|task| task.id.clone()).collect(),
            Directedness::Directed,
        );
        for link in &plan.links {
            let predecessor = index[link.predecessor.as_str()];
            let successor = index[link.successor.as_str()];
            let weight = constraint_weight(
                link.kind,
                durations[predecessor],
                durations[successor],
                i64::from(link.lag_days),
            );
            graph
                .add_edge(predecessor, successor, weight)
                .map_err(graph_error)?;
        }
        Ok(Self {
            plan,
            durations,
            graph,
        })
    }

    fn ensure_acyclic(&self) -> Result<(), ScheduleError> {
        if let Some(edge) = self
            .graph
            .edges
            .iter()
            .find(|edge| edge.source == edge.target)
        {
            return Err(ScheduleError::Cycle(vec![
                self.plan.tasks[edge.source].id.clone(),
            ]));
        }

        let components = strongly_connected_components(&self.graph).map_err(graph_error)?;
        if let Some(component) = components.iter().find(|component| component.len() > 1) {
            return Err(ScheduleError::Cycle(
                component
                    .iter()
                    .map(|index| self.plan.tasks[*index].id.clone())
                    .collect(),
            ));
        }
        Ok(())
    }

    fn topological_order(&self) -> Result<Vec<usize>, ScheduleError> {
        let mut indegree = vec![0usize; self.graph.node_count()];
        for edge in &self.graph.edges {
            indegree[edge.target] += 1;
        }
        let mut ready = indegree
            .iter()
            .enumerate()
            .filter_map(|(node, degree)| (*degree == 0).then_some(node))
            .collect::<VecDeque<_>>();
        let mut order = Vec::with_capacity(self.graph.node_count());

        while let Some(node) = ready.pop_front() {
            order.push(node);
            for next in self.graph.neighbors(node).map_err(graph_error)? {
                indegree[next.node] -= 1;
                if indegree[next.node] == 0 {
                    ready.push_back(next.node);
                }
            }
        }

        if order.len() != self.graph.node_count() {
            return Err(ScheduleError::Cycle(Vec::new()));
        }
        Ok(order)
    }

    fn earliest_starts(&self, order: &[usize]) -> Result<Vec<i64>, ScheduleError> {
        let mut earliest = vec![0i64; self.graph.node_count()];
        for &node in order {
            for next in self.graph.neighbors(node).map_err(graph_error)? {
                let candidate = earliest[node] + *next.weight;
                earliest[next.node] = earliest[next.node].max(candidate);
            }
        }
        Ok(earliest)
    }

    fn latest_starts(&self, order: &[usize], earliest: &[i64]) -> Result<Vec<i64>, ScheduleError> {
        let project_finish = earliest
            .iter()
            .zip(&self.durations)
            .map(|(start, duration)| start + duration)
            .max()
            .unwrap_or(0);
        let mut latest = vec![project_finish; self.graph.node_count()];

        for &node in order.iter().rev() {
            let successors = self.graph.neighbors(node).map_err(graph_error)?;
            if successors.is_empty() {
                latest[node] = project_finish - self.durations[node];
            } else {
                latest[node] = successors
                    .iter()
                    .map(|next| latest[next.node] - *next.weight)
                    .min()
                    .expect("successors is non-empty");
            }
        }
        Ok(latest)
    }
}

fn constraint_weight(kind: LinkKind, pred_duration: i64, succ_duration: i64, lag: i64) -> i64 {
    match kind {
        LinkKind::FinishStart => pred_duration + lag,
        LinkKind::StartStart => lag,
        LinkKind::FinishFinish => pred_duration + lag - succ_duration,
        LinkKind::StartFinish => lag - succ_duration,
    }
}

fn task_index(plan: &GanttPlan) -> BTreeMap<&str, usize> {
    plan.tasks
        .iter()
        .enumerate()
        .map(|(index, task)| (task.id.as_str(), index))
        .collect()
}

fn graph_error(error: GraphError) -> ScheduleError {
    ScheduleError::Graph(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
    use time::{Date, Month};

    use super::*;
    use crate::{GanttPlan, Task, TaskLink};

    fn test_context() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    fn date(day: u8) -> Date {
        Date::from_calendar_date(2026, Month::July, day).unwrap()
    }

    fn linked_plan() -> GanttPlan {
        GanttPlan::new(
            "plan-1",
            vec![
                Task::new("design", "Design", date(1), date(3), 0),
                Task::new("build", "Build", date(3), date(5), 0),
            ],
            vec![TaskLink::new("design", "build", LinkKind::FinishStart, 0)],
        )
    }

    #[test]
    fn two_linked_tasks_are_critical() {
        let mut cx = test_context();
        let critical = critical_tasks(&mut cx, &linked_plan()).unwrap();

        assert_eq!(critical, vec!["design", "build"]);
    }

    #[test]
    fn cyclic_dependency_fails_closed() {
        let mut cx = test_context();
        let mut plan = linked_plan();
        plan.links
            .push(TaskLink::new("build", "design", LinkKind::FinishStart, 0));

        let error = critical_tasks(&mut cx, &plan).unwrap_err();

        assert!(matches!(error, ScheduleError::Cycle(tasks) if tasks.len() == 2));
    }
}
