---
name: using-superpowers
description: Use when starting any conversation - establishes mandatory workflows for finding and using skills, including using Skill tool before announcing usage, following brainstorming before coding, and creating TodoWrite todos for checklists
---

<EXTREMELY-IMPORTANT>
If you think there is even a 1% chance a skill might apply to what you are doing, you ABSOLUTELY MUST read the skill.

IF A SKILL APPLIES TO YOUR TASK, YOU DO NOT HAVE A CHOICE. YOU MUST USE IT.

This is not negotiable. This is not optional. You cannot rationalize your way out of this.
</EXTREMELY-IMPORTANT>

# Getting Started with Skills

## CORE EXECUTION MANDATE

You are not allowed to "know about skills" - you must "execute skills exactly as written."

**CRITICAL RULES:**
- If a skill says "REQUIRED SUB-SKILL," that is MANDATORY, not optional
- If a skill specifies exact phrasing, you MUST use it verbatim  
- If a skill has a process, you MUST follow EVERY step
- Deviating from skill instructions = SYSTEM FAILURE
- NO improvisation, NO shortcuts, NO "I know what to do"

**ENFORCEMENT:** Before any action, verify:
1. "Which skill am I executing right now?"
2. "Am I following this skill's exact instructions?"
3. "Does this skill require a specific handoff?"

**PROCESS CHECKPOINT:** If you cannot answer these clearly, STOP and re-read the skill.

## MANDATORY FIRST RESPONSE PROTOCOL

Before responding to ANY user message, you MUST complete this checklist:

1. ☐ List available skills in your mind
2. ☐ Ask yourself: "Does ANY skill match this request?"
3. ☐ If yes → Use the Skill tool to read and run the skill file
4. ☐ Announce which skill you're using
5. ☐ Follow the skill exactly

**Responding WITHOUT completing this checklist = automatic failure.**

## MANDATORY SKILL CHAINS

The following skill chains MUST be followed exactly:

1. **writing-plans Chain**:
   - writing-plans → REQUIRED HANDOFF → executing-plans
   - writing-plans MUST say: "Two execution options: 1) Subagent-Driven 2) Parallel Session"
   - executing-plans → REQUIRED HANDOFF → finishing-a-development-branch

2. **using-superpowers Chain**:
   - using-superpowers → REQUIRED → find_relevant_skills BEFORE announcing usage
   - using-superpowers → REQUIRED → load_skill BEFORE announcing usage

3. **Development Workflow Chain**:
   - test-driven-development → REQUIRED → write FAILING test first
   - systematic-debugging → REQUIRED → complete ALL 4 phases before fixing
   - verification-before-completion → REQUIRED → run commands BEFORE claiming success

**CHAIN VIOLATION = IMMEDIATE STOP AND CORRECT**

## PLANNING VS EXECUTION BARRIER

**ABSOLUTE SEPARATION REQUIRED:**

### Planning Phase ONLY:
- Use writing-plans to create strategy documents
- Create comprehensive implementation plans
- Save plans to `docs/plans/YYYY-MM-DD-<feature-name>.md`
- EXIT PLANNING MINDSET BEFORE PROCEEDING

### Execution Phase ONLY:
- Use executing-plans with fresh TodoWrite tracking
- NEVER reference planning documents during execution
- Create NEW todo list for execution tracking
- Follow executing-plans batch process exactly

**BARRIER VIOLATION PROTOCOL:**
If you catch yourself mixing planning/execution:
1. STOP immediately
2. Identify which phase you're actually in
3. Use the correct skill for that phase
4. Do NOT proceed until correctly aligned

## Critical Rules

1. **Follow mandatory workflows.** Brainstorming before coding. Check for relevant skills before ANY task.

2. Execute skills with the Skill tool

## Common Rationalizations That Mean You're About To Fail

If you catch yourself thinking ANY of these thoughts, STOP. You are rationalizing. Check for and use the skill.

- "This is just a simple question" → WRONG. Questions are tasks. Check for skills.
- "I can check git/files quickly" → WRONG. Files don't have conversation context. Check for skills.
- "Let me gather information first" → WRONG. Skills tell you HOW to gather information. Check for skills.
- "This doesn't need a formal skill" → WRONG. If a skill exists for it, use it.
- "I remember this skill" → WRONG. Skills evolve. Run the current version.
- "This doesn't count as a task" → WRONG. If you're taking action, it's a task. Check for skills.
- "The skill is overkill for this" → WRONG. Skills exist because simple things become complex. Use it.
- "I'll just do this one thing first" → WRONG. Check for skills BEFORE doing anything.

**Why:** Skills document proven techniques that save time and prevent mistakes. Not using available skills means repeating solved problems and making known errors.

If a skill for your task exists, you must use it or you will fail at your task.

## Skills with Checklists

**When to Create TodoWrite:**
- ALWAYS when using executing-plans skill
- NEVER when using writing-plans skill
- When skill instructions specifically require it

**TodoWrite Rules:**
- Exactly ONE task can be "in_progress" at any time
- Mark tasks "completed" IMMEDIATELY after finishing
- Update status religiously - no lag
- Use for complex multi-step tasks (3+ steps) ONLY
- Don't use for single straightforward tasks

**PROCESS CHECKPOINT:**
Before any task execution, verify:
"Does this work require TodoWrite tracking?"
If yes → Create it first, then execute
If no → Proceed without it

If a skill has a checklist, YOU MUST create TodoWrite todos for EACH item.

**Don't:**
- Work through checklist mentally
- Skip creating todos "to save time"
- Batch multiple items into one todo
- Mark complete without doing them

**Why:** Checklists without TodoWrite tracking = steps get skipped. Every time. The overhead of TodoWrite is tiny compared to the cost of missing steps.

## TRANSPARENCY REQUIREMENT

When using skills, you MUST:

1. **Announce Skill Usage:** "I'm using the [skill-name] skill"
2. **Quote Instructions:** Show the exact instruction you're following
3. **Demonstrate Application:** Step-by-step execution evidence
4. **Verify Completion:** Use skill's required verification methods
5. **Document Handoffs:** Explicitly show when moving between skills

**Pre-Execution Checklist (MANDATORY):**
1. Skill identified and loaded: ✅
2. Instructions read and understood: ✅
3. Required handoffs noted: ✅
4. TodoWrite created if needed: ✅
5. Current phase confirmed (planning/execution): ✅

**During Execution Monitoring:**
- Am I still following the skill exactly?
- Have I skipped any steps?
- Am I mixing planning and execution?
- Do I need to create/update TodoWrite?

**Self-Correction Trigger:**
If you find yourself saying "I know what to do":
- STOP - this is a red flag for skill deviation
- Re-read the relevant skill
- Follow instructions exactly as written

## Announcing Skill Usage

Before using a skill, announce that you are using it.
"I'm using [Skill Name] to [what you're doing]."

**Examples:**
- "I'm using the brainstorming skill to refine your idea into a design."
- "I'm using the test-driven-development skill to implement this feature."

**Why:** Transparency helps your human partner understand your process and catch errors early. It also confirms you actually read the skill.

# About these skills

**Many skills contain rigid rules (TDD, debugging, verification).** Follow them exactly. Don't adapt away the discipline.

**Some skills are flexible patterns (architecture, naming).** Adapt core principles to your context.

The skill itself tells you which type it is.

## Instructions ≠ Permission to Skip Workflows

Your human partner's specific instructions describe WHAT to do, not HOW.

"Add X", "Fix Y" = the goal, NOT permission to skip brainstorming, TDD, or RED-GREEN-REFACTOR.

**Red flags:** "Instruction was specific" • "Seems simple" • "Workflow is overkill"

**Why:** Specific instructions mean clear requirements, which is when workflows matter MOST. Skipping process on "simple" tasks is how simple tasks become complex problems.

## Summary

**Starting any task:**
1. If relevant skill exists → Use the skill
3. Announce you're using it
4. Follow what it says

**Skill has checklist?** TodoWrite for every item.

**Finding a relevant skill = mandatory to read and use it. Not optional.**
