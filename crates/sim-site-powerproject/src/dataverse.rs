//! Dataverse Project Schedule Service operation planning.

use std::collections::BTreeMap;

use sim_lib_gantt::{GanttPlan, Task, TaskLink};

use crate::PowerprojectError;

/// Project for the web target inside a Dataverse environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataverseProjectTarget {
    /// Base Dataverse environment URL.
    pub environment_url: String,
    /// Dataverse project row id.
    pub project_id: String,
}

impl DataverseProjectTarget {
    /// Builds a Dataverse Project target.
    #[must_use]
    pub fn new(environment_url: impl Into<String>, project_id: impl Into<String>) -> Self {
        Self {
            environment_url: environment_url.into(),
            project_id: project_id.into(),
        }
    }
}

/// Dataverse Project Schedule Service action kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataverseAction {
    /// Creates an operation set for a batch of schedule mutations.
    CreateOperationSet,
    /// Adds or updates one project task inside the operation set.
    CreateTask,
    /// Adds or updates one task dependency inside the operation set.
    CreateDependency,
    /// Executes the completed operation set.
    ExecuteOperationSet,
}

impl DataverseAction {
    /// Returns the Project Schedule Service action name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreateOperationSet => "msdyn_CreateOperationSetV1",
            Self::CreateTask | Self::CreateDependency => "msdyn_PssCreateV1",
            Self::ExecuteOperationSet => "msdyn_ExecuteOperationSetV1",
        }
    }
}

/// Planned Dataverse operation with a stable action name and payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataverseOperation {
    /// Action used to submit the operation.
    pub action: DataverseAction,
    /// Operation set id shared by all actions in this update.
    pub operation_set_id: String,
    /// Dataverse logical table name for this operation.
    pub logical_name: String,
    /// String payload fields sent to the action.
    pub payload: BTreeMap<String, String>,
}

impl DataverseOperation {
    fn new(
        action: DataverseAction,
        operation_set_id: String,
        logical_name: impl Into<String>,
        payload: BTreeMap<String, String>,
    ) -> Self {
        Self {
            action,
            operation_set_id,
            logical_name: logical_name.into(),
            payload,
        }
    }
}

/// Plans Project for the web Dataverse operations for a local Gantt plan.
pub fn plan_pss_update(
    plan: &GanttPlan,
    target: &DataverseProjectTarget,
) -> Result<Vec<DataverseOperation>, PowerprojectError> {
    validate_target(target)?;
    let operation_set_id = format!("sim-{}-operation-set", sanitize_token(&plan.id));
    let mut operations = Vec::with_capacity(plan.tasks.len() + plan.links.len() + 2);

    operations.push(operation_set_operation(&operation_set_id, target));
    operations.extend(
        plan.tasks
            .iter()
            .map(|task| task_operation(task, target, &operation_set_id)),
    );
    operations.extend(
        plan.links
            .iter()
            .map(|link| dependency_operation(link, target, &operation_set_id)),
    );
    operations.push(execute_operation_set(&operation_set_id, target));

    Ok(operations)
}

fn validate_target(target: &DataverseProjectTarget) -> Result<(), PowerprojectError> {
    if target.environment_url.trim().is_empty() {
        return Err(PowerprojectError::Dataverse(
            "environment_url must not be empty".to_owned(),
        ));
    }
    if target.project_id.trim().is_empty() {
        return Err(PowerprojectError::Dataverse(
            "project_id must not be empty".to_owned(),
        ));
    }
    Ok(())
}

fn operation_set_operation(
    operation_set_id: &str,
    target: &DataverseProjectTarget,
) -> DataverseOperation {
    let mut payload = base_payload(target);
    payload.insert("OperationSetId".to_owned(), operation_set_id.to_owned());
    DataverseOperation::new(
        DataverseAction::CreateOperationSet,
        operation_set_id.to_owned(),
        "msdyn_operationset",
        payload,
    )
}

fn task_operation(
    task: &Task,
    target: &DataverseProjectTarget,
    operation_set_id: &str,
) -> DataverseOperation {
    let mut payload = base_payload(target);
    payload.insert("OperationSetId".to_owned(), operation_set_id.to_owned());
    payload.insert("Target".to_owned(), "msdyn_projecttask".to_owned());
    payload.insert("msdyn_project@odata.bind".to_owned(), project_bind(target));
    payload.insert("msdyn_subject".to_owned(), task.name.clone());
    payload.insert("sim_task_id".to_owned(), task.id.clone());
    payload.insert("msdyn_scheduledstart".to_owned(), task.start.to_string());
    payload.insert("msdyn_scheduledend".to_owned(), task.finish.to_string());
    payload.insert(
        "msdyn_percentcomplete".to_owned(),
        task.percent_complete.to_string(),
    );
    DataverseOperation::new(
        DataverseAction::CreateTask,
        operation_set_id.to_owned(),
        "msdyn_projecttask",
        payload,
    )
}

fn dependency_operation(
    link: &TaskLink,
    target: &DataverseProjectTarget,
    operation_set_id: &str,
) -> DataverseOperation {
    let mut payload = base_payload(target);
    payload.insert("OperationSetId".to_owned(), operation_set_id.to_owned());
    payload.insert(
        "Target".to_owned(),
        "msdyn_projecttaskdependency".to_owned(),
    );
    payload.insert("msdyn_project@odata.bind".to_owned(), project_bind(target));
    payload.insert(
        "sim_predecessor_task_id".to_owned(),
        link.predecessor.clone(),
    );
    payload.insert("sim_successor_task_id".to_owned(), link.successor.clone());
    payload.insert("msdyn_linktype".to_owned(), link.kind.as_str().to_owned());
    payload.insert("msdyn_lag".to_owned(), link.lag_days.to_string());
    DataverseOperation::new(
        DataverseAction::CreateDependency,
        operation_set_id.to_owned(),
        "msdyn_projecttaskdependency",
        payload,
    )
}

fn execute_operation_set(
    operation_set_id: &str,
    target: &DataverseProjectTarget,
) -> DataverseOperation {
    let mut payload = base_payload(target);
    payload.insert("OperationSetId".to_owned(), operation_set_id.to_owned());
    DataverseOperation::new(
        DataverseAction::ExecuteOperationSet,
        operation_set_id.to_owned(),
        "msdyn_operationset",
        payload,
    )
}

fn base_payload(target: &DataverseProjectTarget) -> BTreeMap<String, String> {
    let mut payload = BTreeMap::new();
    payload.insert(
        "EnvironmentUrl".to_owned(),
        target.environment_url.trim_end_matches('/').to_owned(),
    );
    payload.insert("ProjectId".to_owned(), target.project_id.clone());
    payload
}

fn project_bind(target: &DataverseProjectTarget) -> String {
    format!("/msdyn_projects({})", target.project_id)
}

fn sanitize_token(value: &str) -> String {
    let token: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    token.trim_matches('-').replace("--", "-")
}

#[cfg(test)]
mod tests {
    use sim_lib_gantt::{GanttPlan, LinkKind, Task, TaskLink};
    use time::{Date, Month};

    use super::*;

    fn plan() -> GanttPlan {
        GanttPlan::new(
            "Build Plan",
            vec![
                Task::new(
                    "design",
                    "Design",
                    Date::from_calendar_date(2026, Month::July, 1).unwrap(),
                    Date::from_calendar_date(2026, Month::July, 3).unwrap(),
                    25,
                ),
                Task::new(
                    "build",
                    "Build",
                    Date::from_calendar_date(2026, Month::July, 4).unwrap(),
                    Date::from_calendar_date(2026, Month::July, 8).unwrap(),
                    0,
                ),
            ],
            vec![TaskLink::new("design", "build", LinkKind::FinishStart, 0)],
        )
    }

    #[test]
    fn dataverse_mapping_creates_operation_set_tasks_and_dependencies() {
        let target = DataverseProjectTarget::new("https://org.crm.dynamics.com/", "project-1");

        let operations = plan_pss_update(&plan(), &target).unwrap();

        assert_eq!(operations.len(), 5);
        assert_eq!(operations[0].action, DataverseAction::CreateOperationSet);
        assert_eq!(operations[0].action.as_str(), "msdyn_CreateOperationSetV1");
        assert_eq!(operations[4].action, DataverseAction::ExecuteOperationSet);
        assert!(
            operations
                .iter()
                .any(|operation| operation.logical_name == "msdyn_projecttask"
                    && operation.payload["msdyn_subject"] == "Design")
        );
        assert!(operations.iter().any(|operation| operation.logical_name
            == "msdyn_projecttaskdependency"
            && operation.payload["sim_predecessor_task_id"] == "design"
            && operation.payload["sim_successor_task_id"] == "build"));
        assert!(
            operations
                .iter()
                .all(|operation| operation.operation_set_id == "sim-build-plan-operation-set")
        );
    }

    #[test]
    fn dataverse_target_requires_project_id() {
        let target = DataverseProjectTarget::new("https://org.crm.dynamics.com", "");

        let error = plan_pss_update(&plan(), &target).unwrap_err();

        assert!(error.to_string().contains("project_id"));
    }
}
