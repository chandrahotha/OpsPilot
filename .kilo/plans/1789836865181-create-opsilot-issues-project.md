# Plan: Create "OpsPilot_issues" GitHub Project V2

## Goal
Create a GitHub Projects V2 named "OpsPilot_issues" in the chandrahotha account/organization and populate it with all pending tasks from the OpsPilot roadmap and open issues. The project items should **dynamically sync** with the main repo - when issues are closed/addressed in the repo, corresponding project items should automatically close/update.

## Prerequisites
- Add `project` scope to GitHub token: `gh auth refresh -h github.com -s project`
- Verify access to create projects under `chandrahotha` user

## Tasks

### 1. Add project scope to token
```bash
gh auth refresh -h github.com -s project
```

### 2. Create Projects V2 "OpsPilot_issues"
```bash
gh project create --owner chandrahotha --title "OpsPilot_issues" --format json
```
Capture the project number/ID for subsequent steps.

### 3. Add custom fields to project
Create fields matching roadmap structure:
- Phase (single select): Phase 4, Phase 5, Phase 6, Phase 7, Phase 8, Phase 9, Phase 10, Phase 11, Phase 12
- Status (single select): Planned, In Progress, Done, Blocked
- Priority (single select): High, Medium, Low
- Type (single select): Feature, Infrastructure, Documentation, Testing

### 4. Populate project with roadmap items
Add each roadmap phase as project items:

**Phase 4 - Process Lifecycle** (from open issue #1)
- Start command implementation
- Stop command implementation
- Restart command implementation
- Status command implementation

**Phase 5 - Docker Operations**
- Docker compose up/down
- Container status
- Logs access
- Image management

**Phase 6 - Database Operations**
- Prisma migrate
- Prisma seed
- Prisma reset (with confirmation)
- Django/Alembic migrations
- PostgreSQL operations

**Phase 7 - Port Management**
- Per-port process ownership
- Safe port changes
- Port conflict resolution

**Phase 8 - Diagnostics**
- Database reachability checks
- Service reachability checks
- Health check endpoints

**Phase 9 - Logs**
- Log aggregation
- Log filtering/search
- Log export

**Phase 10 - AI Layer** (optional)
- AI-assisted diagnostics
- Natural language queries
- Smart suggestions

**Phase 11 - Packaging**
- Windows installer (MSI)
- macOS package (DMG/PKG)
- Linux packages (AppImage, deb, rpm)

**Phase 12 - Release**
- npm publish pipeline
- GitHub Releases automation
- Changelog generation
- Version bumping

### 5. Link existing issue
Add existing issue #1 "Add Process lifecycle: start, stop, restart, status" to the project

### 6. Configure dynamic sync (auto-update project items when repo issues change)
**Option A: GitHub Projects V2 Built-in Automation** (preferred - no code needed)
- Enable "Auto-close items when linked issues are closed" in project settings
- Enable "Auto-add issues from repo" filter for `chandrahotha/OpsPilot`
- Configure field sync: when issue labels change, update project fields

**Option B: GitHub Actions Workflow** (fallback if built-in insufficient)
Create `.github/workflows/sync-project.yml`:
```yaml
name: Sync Project Items
on:
  issues:
    types: [opened, closed, reopened, labeled, unlabeled, edited]
  project_card:
    types: [moved, deleted]
jobs:
  sync:
    runs-on: ubuntu-latest
    permissions:
      issues: read
      projects: write
    steps:
      - uses: actions/github-script@v7
        with:
          script: |
            // Sync logic: find project items linked to changed issue, update status
```

### 7. Verify project
```bash
gh project view <project-number> --owner chandrahotha --web
```

## Validation
- Project "OpsPilot_issues" exists under chandrahotha
- All 9 phases (4-12) represented as items
- Custom fields configured
- Existing issue #1 linked
- Items have appropriate phase, status, priority, type fields set
- **Dynamic sync verified**: Close issue #1 in repo → project item auto-closes

## Notes
- Project will be private (matching OpsPilot_Proj_Logs visibility)
- Items created as draft issues initially, can be converted to issues later
- Field values should reflect current roadmap status (Phase 4 = Next/In Progress, Phases 5-12 = Planned)
- Dynamic sync prefers built-in GitHub Projects automation over custom Actions to minimize maintenance