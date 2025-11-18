# Code Review Compliance Checklist

This checklist ensures that code reviews are performed systematically for all code changes in the KimiChat project.

## 🚨 MANDATORY Review Triggers

A code review **MUST** be performed when any of the following conditions are met:

- [ ] **Any commit adds more than 50 lines of code**
- [ ] **More than 3 files are changed in a single commit**
- [ ] **Changes affect core architecture or public APIs**
- [ ] **Security-related changes are made**
- [ ] **Performance-critical code is modified**
- [ ] **New dependencies are added**
- [ ] **Test coverage is decreased**
- [ ] **Any build or test failures occur**

## 📋 Pre-Review Requirements

Before requesting a code review, ensure:

### Code Quality
- [ ] `cargo clippy` passes with no warnings
- [ ] `cargo fmt` shows no formatting issues
- [ ] All tests pass: `cargo test --all-features`
- [ ] No TODO or FIXME comments left in production code
- [ ] Documentation is updated for any public API changes

### Testing
- [ ] New features have corresponding tests
- [ ] Bug fixes include regression tests
- [ ] Test coverage is maintained or improved
- [ ] Integration tests are updated if needed

### Documentation
- [ ] README.md is updated for user-facing changes
- [ ] Inline documentation (doc comments) is complete
- [ ] Architecture docs are updated for structural changes
- [ ] Change log is updated for significant features

## 🔍 Code Review Process

### 1. Request Review
```bash
# Get the SHAs for your changes
BASE_SHA=$(git rev-parse HEAD~1)  # or appropriate base
HEAD_SHA=$(git rev-parse HEAD)

# Use the requesting-code-review skill
./scripts/verify-code-review.sh --base-sha $BASE_SHA --head-sha $HEAD_SHA
```

### 2. Review Categories

#### **Critical Issues** (Must fix before proceeding)
- [ ] Security vulnerabilities
- [ ] Data corruption risks
- [ ] Memory safety issues
- [ ] Breaking changes to public APIs
- [ ] Test failures or broken builds

#### **Important Issues** (Should fix before merging)
- [ ] Performance regressions
- [ ] Architecture violations
- [ ] Code that's difficult to understand
- [ ] Missing error handling
- [ ] Inconsistent naming or patterns

#### **Minor Issues** (Note for future work)
- [ ] Style inconsistencies
- [ ] Minor optimizations
- [ ] Documentation improvements
- [ ] Code that could be more DRY

### 3. Review Completion

#### Reviewer Responsibilities:
- [ ] All Critical issues are identified
- [ ] All Important issues are documented
- [ ] Code logic and architecture are verified
- [ ] Test coverage is adequate
- [ ] Documentation is complete

#### Author Responsibilities:
- [ ] All Critical issues are fixed
- [ ] All Important issues are addressed or justified
- [ ] Changes are re-verified after fixes
- [ ] Commit messages include review reference

## 📊 Review Tracking

### Commit Message Format
```
feat: add subagent tools for task delegation

Implements single-agent mode with task delegation capabilities.

Code-Review: Reviewed-by-@[reviewer] | Critical: 0 | Important: 2 | Minor: 1
Fixes: #123
```

### Review Tags
- `Code-Review:` - Indicates review completion
- `Reviewed-by:` - Credits the reviewer
- `Critical: N` - Number of critical issues found
- `Important: N` - Number of important issues found
- `Minor: N` - Number of minor issues found

### Git Notes (Optional)
```bash
# Add review notes to a commit
git notes add -m "Review completed by [reviewer]
- Critical: 0
- Important: 2 (add error handling, improve docs)
- Minor: 1 (optimize loop)
Approved: Yes" HEAD
```

## 🔧 Automated Verification

The `scripts/verify-code-review.sh` script automatically:

1. **Checks recent commits** for review markers
2. **Analyzes change complexity** to determine if review is needed
3. **Runs code quality checks** (clippy, fmt, tests)
4. **Identifies commits requiring review**
5. **Prevents unreviewed code from being merged**

### Integration Points

#### Pre-commit Hook
```bash
#!/bin/sh
# .git/hooks/pre-commit
./scripts/verify-code-review.sh --no-recent --base-sha HEAD
```

#### CI/CD Pipeline
```yaml
# Example GitHub Actions step
- name: Verify Code Review
  run: ./scripts/verify-code-review.sh --days 1
```

#### Git Workflow Integration
```bash
# Before merging to main
git checkout main
git pull origin main
git merge feature-branch
./scripts/verify-code-review.sh --base-sha origin/main --head-sha HEAD
```

## 📈 Review Metrics

Track these metrics to ensure review quality:

- [ ] **Review Coverage**: % of commits reviewed
- [ ] **Review Latency**: Average time from commit to review
- [ ] **Issue Detection**: Number of issues found per review
- [ ] **Fix Rate**: % of identified issues that are fixed
- [ ] **Recurrence**: Rate of similar issues across reviews

## 🚫 Anti-Patterns to Avoid

### Never Skip Review Because:
- [ ] "It's just a small change"
- [ ] "I'm confident it's correct"
- [ ] "It's an urgent fix"
- [ ] "The tests pass"
- [ ] "I'll review it myself"

### Review Anti-Patterns:
- [ ] **Superficial reviews** - only checking style
- [ ] **Rubber-stamping** - approving without thorough analysis
- [ ] **Delaying reviews** - letting PRs sit for days
- [ ] **Ignoring feedback** - not addressing review comments
- [ ] **Personal feedback** - focusing on person instead of code

## ✅ Success Criteria

A code review is considered complete when:

1. **All Critical issues are resolved**
2. **All Important issues are addressed or justified**
3. **Code quality checks pass**
4. **Tests are comprehensive and passing**
5. **Documentation is complete**
6. **Both author and reviewer agree the code is ready**

## 🆘 Getting Help

If you're unsure about the review process:

1. **Check the skills**: Use `requesting-code-review` skill
2. **Ask for help**: Request review from senior developers
3. **Reference examples**: Look at previous approved reviews
4. **Use templates**: Follow the code-reviewer.md format
5. **Escalate if needed**: Contact project maintainers for guidance

Remember: **Code reviews are about learning and improving, not about judgment.** Every review is an opportunity to make the codebase better for everyone.