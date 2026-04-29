pub mod deploy;
pub mod github;
pub mod install;
pub mod model;
pub mod registry;
pub mod service;

pub use model::{apply_github_proxy_env, Skill, SkillIndex, SkillSource};
pub use registry::{create_local_skill, list_skill_files, read_skill_content, SkillRegistry};
pub use service::ServiceResult;
