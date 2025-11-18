---
name: executing-plans
description: Use when partner provides a complete implementation plan to execute in controlled batches with review checkpoints - loads plan, reviews critically, executes tasks in batches, reports for review between batches
---

# Executing Plans

## Overview

Load plan, review critically, execute tasks in batches, report for review between batches.

**Core principle:** Batch execution with checkpoints for architect review.

**MANDATORY PRE-EXECUTION:**
- You MUST announce: "I'm using the executing-plans skill to implement this plan."
- You MUST create TodoWrite tracking for execution phase
- You MUST verify you're in EXECUTION phase, not planning phase

## The Process

### Step 1: Load and Review Plan
1. **MANDATORY:** Create TodoWrite for task tracking BEFORE starting
2. Read plan file
3. Review critically - identify any questions or concerns about the plan
4. If concerns: Raise them with your human partner before starting
5. If no concerns: Proceed with TodoWrite tracking and batch execution

### Step 2: Execute Batch
**Default: First 3 tasks**

For each task:
1. **MANDATORY:** Mark as in_progress in TodoWrite
2. Follow each step exactly (plan has bite-sized steps)
3. **MANDATORY:** Quote the instruction you're following
4. **MANDATORY:** Run verifications as specified
5. **MANDATORY:** Mark as completed in TodoWrite immediately

### Step 3: Report
When batch complete:
- Show what was implemented
- Show verification output
- Say: "Ready for feedback."

### Step 4: Continue
Based on feedback:
- Apply changes if needed
- Execute next batch
- Repeat until complete

### Step 5: Complete Development

After all tasks complete and verified:
- **MANDATORY:** Announce: "I'm using the finishing-a-development-branch skill to complete this work."
- **MANDATORY SUB-SKILL:** Use superpowers:finishing-a-development-branch
- Follow that skill to verify tests, present options, execute choice

## When to Stop and Ask for Help

**STOP executing immediately when:**
- Hit a blocker mid-batch (missing dependency, test fails, instruction unclear)
- Plan has critical gaps preventing starting
- You don't understand an instruction
- Verification fails repeatedly
- You find yourself skipping steps or not following the plan exactly

**Ask for clarification rather than guessing.**

## When to Revisit Earlier Steps

**Return to Review (Step 1) when:**
- Partner updates the plan based on your feedback
- Fundamental approach needs rethinking

**Don't force through blockers** - stop and ask.

## PROCESS FIDELITY ENFORCEMENT

**MANDATORY During Execution:**
- Quote the exact step you're following from the plan
- Show the work you're doing to complete that step
- Run the verification command exactly as specified
- Update TodoWrite status immediately
- Never skip steps or take shortcuts

**Self-Monitoring Questions (Ask constantly):**
- "Am I following the plan step exactly?"
- "Did I skip any verification?"
- "Is my TodoWrite tracking current?"
- "Am I mixing planning and execution contexts?"

**If any answer is uncertain → STOP → RE-READ PLAN → PROCEED CORRECTLY**

## Remember
- Review plan critically first
- Follow plan steps exactly
- Don't skip verifications
- Reference skills when plan says to
- Between batches: just report and wait
- Stop when blocked, don't guess
- TodoWrite tracking is MANDATORY
- Process fidelity is MANDATORY
