use serde::Serialize;

/// Events pushed to all connected browser tabs via SSE.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiEvent {
    Toast { level: ToastLevel, msg: String },
    TaskStarted { id: u64, label: String },
    TaskProgress { id: u64, note: String },
    TaskFinished { id: u64, ok: bool, msg: String },
    Invalidate { resource: ResourceKind },
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToastLevel {
    Info,
    Success,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceKind {
    Skills,
    Mcp,
    Secrets,
    Projects,
}

impl ResourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Skills => "skills",
            Self::Mcp => "mcp",
            Self::Secrets => "secrets",
            Self::Projects => "projects",
        }
    }
}
