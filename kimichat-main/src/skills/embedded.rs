/// Embedded skills that are compiled into the binary
/// These are always available even if the skills/ directory doesn't exist
use std::collections::HashMap;

pub struct EmbeddedSkill {
    pub name: &'static str,
    pub content: &'static str,
}

/// All skills embedded at compile time
pub fn get_embedded_skills() -> HashMap<&'static str, &'static str> {
    let mut skills = HashMap::new();

    // Embed all SKILL.md files from skills/ directory
    skills.insert("brainstorming", include_str!("../../../skills/brainstorming/SKILL.md"));
    skills.insert("condition-based-waiting", include_str!("../../../skills/condition-based-waiting/SKILL.md"));
    skills.insert("defense-in-depth", include_str!("../../../skills/defense-in-depth/SKILL.md"));
    skills.insert("dispatching-parallel-agents", include_str!("../../../skills/dispatching-parallel-agents/SKILL.md"));
    skills.insert("executing-plans", include_str!("../../../skills/executing-plans/SKILL.md"));
    skills.insert("finishing-a-development-branch", include_str!("../../../skills/finishing-a-development-branch/SKILL.md"));
    skills.insert("receiving-code-review", include_str!("../../../skills/receiving-code-review/SKILL.md"));
    skills.insert("requesting-code-review", include_str!("../../../skills/requesting-code-review/SKILL.md"));
    skills.insert("root-cause-tracing", include_str!("../../../skills/root-cause-tracing/SKILL.md"));
    skills.insert("sharing-skills", include_str!("../../../skills/sharing-skills/SKILL.md"));
    skills.insert("subagent-driven-development", include_str!("../../../skills/subagent-driven-development/SKILL.md"));
    skills.insert("systematic-debugging", include_str!("../../../skills/systematic-debugging/SKILL.md"));
    skills.insert("test-driven-development", include_str!("../../../skills/test-driven-development/SKILL.md"));
    skills.insert("testing-anti-patterns", include_str!("../../../skills/testing-anti-patterns/SKILL.md"));
    skills.insert("testing-skills-with-subagents", include_str!("../../../skills/testing-skills-with-subagents/SKILL.md"));
    skills.insert("using-git-worktrees", include_str!("../../../skills/using-git-worktrees/SKILL.md"));
    skills.insert("using-superpowers", include_str!("../../../skills/using-superpowers/SKILL.md"));
    skills.insert("verification-before-completion", include_str!("../../../skills/verification-before-completion/SKILL.md"));
    skills.insert("writing-plans", include_str!("../../../skills/writing-plans/SKILL.md"));
    skills.insert("writing-skills", include_str!("../../../skills/writing-skills/SKILL.md"));

    skills
}
