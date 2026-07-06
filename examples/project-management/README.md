# project-management

A Linear/Jira-style issue tracker.

**Domain:** Engineering project management — organizations, teams, projects, issues, sprints, labels.

**Provenance:** Synthetic (invented from data-modeling knowledge).

---

## Models and key relationships

| Model | Key fields | Relations |
|-------|-----------|-----------|
| `Organization` | `slug ^&string` | many `Team`, many `User` |
| `User` | `email &string` | `?Organization`, many assigned `Issue`, many `IssueComment` |
| `Team` | `name` | `*Organization`, many `Project` |
| `Project` | `status @default(active)` | `*Team`, many `Issue`, `Sprint`, `Label` |
| `Issue` | `status @default(backlog)`, `priority @default(medium)` | `*Project`, `?User` assignee, `*User` reporter, `?Sprint`, self-ref `?Issue` parent, M2M `Label`, many `IssueComment` |
| `Sprint` | `start_at`, `end_at`, `is_active` | `*Project`, many `Issue` |
| `Label` | `name`, `color` | `*Project`, M2M `Issue` |
| `IssueComment` | `body` | `*Issue`, `*User` author |

**Relation summary:**
- `Issue.labels: [Label]` + `Label.issues: [Issue]` — parser auto-detects M2M
- `Issue.parent: ?Issue` — self-referential sub-issue hierarchy
- `Issue.assignee: ?User` — optional FK (unassigned issues)
- `Issue.reporter: *User` — required FK (every issue has a reporter)
- `Issue.sprint: ?Sprint` — issues may be outside a sprint (backlog)

---

## Grammar features showcased

- Three-level hierarchy: `Organization → Team → Project → Issue`
- Self-referential optional FK on `Issue.parent: ?Issue`
- Both optional FK (`?User` assignee) and required FK (`*User` reporter) on the same model
- Bidirectional M2M: `Issue.labels: [Label]` + `Label.issues: [Issue]`
- Multiple composite indexes on one model: `@index(status, created_at)` and `@index(status, priority)` on `Issue`
- `@default(backlog)`, `@default(medium)`, `@default(false)` — unquoted identifier defaults
- `timestamp` for sprint date range (`start_at`, `end_at`)
- `bool @default(false)` on `Sprint.is_active`
- `string?` nullable optional fields (`description`, `notes`)
