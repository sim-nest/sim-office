//! Gantt plan records shared by local stores and project codecs.

use time::Date;

/// Open office document kind string for schedule plans.
pub const GANTT_DOC_KIND: &str = "gantt";

/// A local schedule plan with tasks and directed dependency links.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GanttPlan {
    /// Stable plan identifier.
    pub id: String,
    /// Tasks in display and persistence order.
    pub tasks: Vec<Task>,
    /// Dependency links in display and persistence order.
    pub links: Vec<TaskLink>,
}

impl GanttPlan {
    /// Creates a plan from its stable id, tasks, and dependency links.
    pub fn new(id: impl Into<String>, tasks: Vec<Task>, links: Vec<TaskLink>) -> Self {
        Self {
            id: id.into(),
            tasks,
            links,
        }
    }

    /// Returns the task with `id`, when present.
    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }
}

/// A scheduled task with exact local dates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    /// Stable task identifier inside the plan.
    pub id: String,
    /// Human-facing task name.
    pub name: String,
    /// Inclusive local start date.
    pub start: Date,
    /// Local finish date.
    pub finish: Date,
    /// Completion percentage from 0 through 100.
    pub percent_complete: u8,
}

impl Task {
    /// Creates a task with exact start and finish dates.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        start: Date,
        finish: Date,
        percent_complete: u8,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            start,
            finish,
            percent_complete,
        }
    }

    /// Returns the task duration in whole local days.
    pub fn duration_days(&self) -> i32 {
        self.finish.to_julian_day() - self.start.to_julian_day()
    }
}

/// Dependency relationship between two Gantt tasks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// The successor starts after the predecessor finishes.
    FinishStart,
    /// The successor starts after the predecessor starts.
    StartStart,
    /// The successor finishes after the predecessor finishes.
    FinishFinish,
    /// The successor finishes after the predecessor starts.
    StartFinish,
}

impl LinkKind {
    /// Stable schema token for this link kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FinishStart => "finish-start",
            Self::StartStart => "start-start",
            Self::FinishFinish => "finish-finish",
            Self::StartFinish => "start-finish",
        }
    }

    /// Parses a stable schema token.
    pub fn from_token(value: &str) -> Option<Self> {
        match value {
            "finish-start" => Some(Self::FinishStart),
            "start-start" => Some(Self::StartStart),
            "finish-finish" => Some(Self::FinishFinish),
            "start-finish" => Some(Self::StartFinish),
            _ => None,
        }
    }
}

/// A typed dependency edge between two tasks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskLink {
    /// Task id of the predecessor.
    pub predecessor: String,
    /// Task id of the successor.
    pub successor: String,
    /// Dependency constraint kind.
    pub kind: LinkKind,
    /// Lag in whole local days. Negative values represent lead.
    pub lag_days: i32,
}

impl TaskLink {
    /// Creates a dependency link between two task ids.
    pub fn new(
        predecessor: impl Into<String>,
        successor: impl Into<String>,
        kind: LinkKind,
        lag_days: i32,
    ) -> Self {
        Self {
            predecessor: predecessor.into(),
            successor: successor.into(),
            kind,
            lag_days,
        }
    }
}
