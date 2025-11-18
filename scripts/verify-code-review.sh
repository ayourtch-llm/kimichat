#!/bin/bash

# Code Review Verification Script
# Ensures code reviews are performed before code is considered complete

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
BASE_SHA="HEAD~1"
HEAD_SHA="HEAD"
REQUIRE_REVIEW=true
CHECK_RECENT=true
DAYS_TO_CHECK=7

# Function to print usage
usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --base-sha SHA    Base commit SHA for comparison (default: HEAD~1)"
    echo "  --head-sha SHA    Head commit SHA for comparison (default: HEAD)"
    echo "  --no-require     Skip review requirement check"
    echo "  --no-recent      Skip recent commits check"
    echo "  --days N         Number of days to check for recent commits (default: 7)"
    echo "  --help           Show this help message"
    exit 1
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --base-sha)
            BASE_SHA="$2"
            shift 2
            ;;
        --head-sha)
            HEAD_SHA="$2"
            shift 2
            ;;
        --no-require)
            REQUIRE_REVIEW=false
            shift
            ;;
        --no-recent)
            CHECK_RECENT=false
            shift
            ;;
        --days)
            DAYS_TO_CHECK="$2"
            shift 2
            ;;
        --help)
            usage
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

# Function to check if a commit has been reviewed
check_commit_reviewed() {
    local commit_sha="$1"
    local commit_message=$(git log --format=%B -n 1 "$commit_sha")
    
    # Look for review markers in commit message
    if echo "$commit_message" | grep -qi "reviewed-by:\|code-review:\|cr:"; then
        return 0
    fi
    
    # Check for review tags in git notes
    if git notes show "$commit_sha" 2>/dev/null | grep -qi "reviewed\|approved"; then
        return 0
    fi
    
    # Check for corresponding review branch or PR
    local commit_date=$(git log -1 --format=%ct "$commit_sha")
    local review_cutoff=$((commit_date + 86400)) # 24 hours after commit
    
    # Look for review activity after the commit
    if git log --since="$commit_date" --until="$review_cutoff" --grep="review.*$commit_sha\|$commit_sha.*review" --oneline | head -1; then
        return 0
    fi
    
    return 1
}

# Function to get commits that need review
get_unreviewed_commits() {
    local since_date=$(date -d "$DAYS_TO_CHECK days ago" --iso-8601)
    
    git log --since="$since_date" --format="%H" | while read -r commit; do
        if ! check_commit_reviewed "$commit"; then
            echo "$commit"
        fi
    done
}

# Function to check code quality metrics
check_code_quality() {
    local base_sha="$1"
    local head_sha="$2"
    
    echo -e "${BLUE}Checking code quality metrics...${NC}"
    
    # Check for clippy warnings
    if ! cargo clippy --all-targets --all-features -- -D warnings; then
        echo -e "${RED}❌ Clippy found warnings or errors${NC}"
        return 1
    fi
    echo -e "${GREEN}✅ Clippy passed${NC}"
    
    # Check formatting
    if ! cargo fmt --all -- --check; then
        echo -e "${RED}❌ Code formatting issues found${NC}"
        return 1
    fi
    echo -e "${GREEN}✅ Code formatting correct${NC}"
    
    # Run tests
    if ! cargo test --all-features; then
        echo -e "${RED}❌ Tests failed${NC}"
        return 1
    fi
    echo -e "${GREEN}✅ All tests passed${NC}"
    
    return 0
}

# Function to analyze changes for complexity
analyze_changes() {
    local base_sha="$1"
    local head_sha="$2"
    
    echo -e "${BLUE}Analyzing changes complexity...${NC}"
    
    # Get change statistics
    local stats=$(git diff --stat "$base_sha".."$head_sha")
    
    # Handle case where there are no changes
    if [[ -z "$stats" ]] || git diff --quiet "$base_sha".."$head_sha"; then
        echo "No changes detected"
        return 0
    fi
    
    # Parse statistics more robustly
    local files_changed=0
    local insertions=0
    local deletions=0
    
    # Parse statistics more robustly - use git summary format
    local summary=$(git diff --shortstat "$base_sha".."$head_sha")
    local files_changed=0
    local insertions=0
    local deletions=0
    
    if [[ -n "$summary" ]]; then
        # Parse format like: "11 files changed, 1224 insertions(+), 15 deletions(-)"
        files_changed=$(echo "$summary" | sed 's/^\s*\([0-9]\+\)\s.*$/\1/')
        insertions=$(echo "$summary" | sed 's/.*,\s*\([0-9]\+\)\s*insertion.*/\1/' | head -1)
        deletions=$(echo "$summary" | sed 's/.*,\s*\([0-9]\+\)\s*deletion.*/\1/' | tail -1)
        
        # Handle case where numbers might be empty
        [[ -z "$files_changed" ]] && files_changed=0
        [[ -z "$insertions" ]] && insertions=0  
        [[ -z "$deletions" ]] && deletions=0
    fi
    
    echo "Files changed: $files_changed"
    echo "Insertions: $insertions"
    echo "Deletions: $deletions"
    
    # Check if changes are extensive enough to require review
    local total_changes=$((insertions + deletions))
    if [[ $total_changes -gt 100 ]] || [[ $files_changed -gt 5 ]]; then
        echo -e "${YELLOW}⚠️  Large changes detected ($total_changes lines in $files_changed files)${NC}"
        echo "These changes should definitely be reviewed."
        return 1
    fi
    
    return 0
}

# Function to dispatch code reviewer agent
dispatch_code_reviewer() {
    local base_sha="$1"
    local head_sha="$2"
    
    echo -e "${BLUE}Dispatching code reviewer agent...${NC}"
    
    # Get commit info
    local commit_info=$(git log --oneline "$base_sha".."$head_sha")
    local commit_count=$(echo "$commit_info" | wc -l)
    
    # Create review request
    local review_request=$(cat <<EOF
Code Review Request

**Commits to review:**
$commit_info

**Total commits:** $commit_count

**Base SHA:** $base_sha
**Head SHA:** $head_sha

**Changes summary:**
$(git diff --stat "$base_sha".."$head_sha")

**Please review these changes for:**
- Code quality and best practices
- Architecture and design patterns
- Test coverage and quality
- Performance implications
- Security considerations
- Documentation completeness

**Use the requesting-code-review skill to perform a thorough review.**
EOF
)
    
    echo "$review_request"
    
    # Here you would typically dispatch to your code reviewer agent
    # For now, we'll just indicate that a review should be performed
    echo -e "${YELLOW}⚠️  Manual code review required for these changes${NC}"
    return 1
}

# Main execution
main() {
    echo -e "${BLUE}Code Review Verification${NC}"
    echo "=========================="
    
    # Check if we're in a git repository
    if ! git rev-parse --git-dir > /dev/null 2>&1; then
        echo -e "${RED}❌ Not in a git repository${NC}"
        exit 1
    fi
    
    # Validate SHAs
    if ! git cat-file -e "$BASE_SHA" 2>/dev/null; then
        echo -e "${RED}❌ Invalid base SHA: $BASE_SHA${NC}"
        exit 1
    fi
    
    if ! git cat-file -e "$HEAD_SHA" 2>/dev/null; then
        echo -e "${RED}❌ Invalid head SHA: $HEAD_SHA${NC}"
        exit 1
    fi
    
    echo "Base SHA: $BASE_SHA"
    echo "Head SHA: $HEAD_SHA"
    echo ""
    
    # Check for unreviewed recent commits
    if [[ "$CHECK_RECENT" == "true" ]]; then
        echo -e "${BLUE}Checking for unreviewed commits in the last $DAYS_TO_CHECK days...${NC}"
        
        local unreviewed_commits=$(get_unreviewed_commits)
        if [[ -n "$unreviewed_commits" ]]; then
            echo -e "${RED}❌ Found unreviewed commits:${NC}"
            echo "$unreviewed_commits" | while read -r commit; do
                echo "  - $commit: $(git log --format=%s -n 1 "$commit")"
            done
            echo ""
        else
            echo -e "${GREEN}✅ All recent commits have been reviewed${NC}"
            echo ""
        fi
    fi
    
    # Analyze changes between base and head
    if git merge-base --is-ancestor "$BASE_SHA" "$HEAD_SHA" 2>/dev/null; then
        echo -e "${BLUE}Analyzing changes between $BASE_SHA and $HEAD_SHA...${NC}"
        
        # Check if there are any changes
        if git diff --quiet "$BASE_SHA".."$HEAD_SHA"; then
            echo -e "${GREEN}✅ No changes to review${NC}"
            exit 0
        fi
        
        # Analyze complexity
        analyze_changes "$BASE_SHA" "$HEAD_SHA"
        local complexity_result=$?
        
        # Check code quality
        check_code_quality "$BASE_SHA" "$HEAD_SHA"
        local quality_result=$?
        
        # Determine if review is required
        if [[ "$REQUIRE_REVIEW" == "true" ]] || [[ $complexity_result -ne 0 ]] || [[ $quality_result -ne 0 ]]; then
            dispatch_code_reviewer "$BASE_SHA" "$HEAD_SHA"
            local review_result=$?
            
            if [[ $review_result -ne 0 ]]; then
                echo -e "${RED}❌ Code review required before proceeding${NC}"
                exit 1
            fi
        fi
        
        echo -e "${GREEN}✅ Code review verification passed${NC}"
    else
        echo -e "${YELLOW}⚠️  Commits are not in a direct ancestry relationship${NC}"
        echo "Cannot perform diff-based review. Please check individual commits."
        exit 1
    fi
}

# Run main function
main "$@"