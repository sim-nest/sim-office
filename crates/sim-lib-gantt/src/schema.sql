PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS gantt_plans (
    id TEXT PRIMARY KEY NOT NULL
);

CREATE TABLE IF NOT EXISTS gantt_tasks (
    plan_id TEXT NOT NULL,
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    start_julian INTEGER NOT NULL,
    finish_julian INTEGER NOT NULL,
    percent_complete INTEGER NOT NULL CHECK (percent_complete >= 0 AND percent_complete <= 100),
    position INTEGER NOT NULL,
    PRIMARY KEY (plan_id, id),
    FOREIGN KEY (plan_id) REFERENCES gantt_plans(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS gantt_links (
    plan_id TEXT NOT NULL,
    predecessor TEXT NOT NULL,
    successor TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('finish-start', 'start-start', 'finish-finish', 'start-finish')),
    lag_days INTEGER NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (plan_id, position),
    FOREIGN KEY (plan_id) REFERENCES gantt_plans(id) ON DELETE CASCADE,
    FOREIGN KEY (plan_id, predecessor) REFERENCES gantt_tasks(plan_id, id) ON DELETE CASCADE,
    FOREIGN KEY (plan_id, successor) REFERENCES gantt_tasks(plan_id, id) ON DELETE CASCADE
);
