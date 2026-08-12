# code-hosting

Self-hosted Git service with repositories, issue tracking, pull requests, and organizations.

**Domain:** Git / code hosting + issue tracking  
**Provenance:** Adapted from [Gitea](https://github.com/go-gitea/gitea) data model — MIT License. No verbatim structure copied; reimagined idiomatically in `.forge`.

## Models (11)

| Model | Key fields | Relationships |
|---|---|---|
| `User` | username (unique+indexed), email (unique) | owns Repositories; O2M OrgMember, Star, Comment; M2M Team |
| `Organization` | name (unique+indexed), website | O2M OrgMember, Repository, Team |
| `OrgMember` | role, is_public | explicit join: `user: *User` + `org: *Organization`; composite @index(user, org) |
| `Repository` | name (indexed), is_private, is_fork | `owner: *User`, `org: ?Organization`, `fork_parent: ?Repository` (self-ref); O2M Issue, Milestone, Label, Star |
| `Team` | name, access_mode | `org: *Organization`; M2M members↔User |
| `Star` | created_at | explicit join: `user: *User` + `repo: *Repository`; composite @index(user, repo) |
| `Issue` | index_num (indexed), title, is_closed, is_pull | `repo: *Repository`, `poster: *User`, `milestone: ?Milestone`; O2M Comment; M2M Label |
| `PullRequest` | head_branch, base_branch, has_merged, status | `issue: *Issue` (1:1 extension), `head_repo: *Repository`, `base_repo: *Repository` (two FKs to same model) |
| `Comment` | content | `issue: *Issue`, `poster: *User`; composite @index(issue, created_at) |
| `Label` | name, color (`string`, exactly 7 chars) | `repo: *Repository`; M2M issues↔Issue |
| `Milestone` | title, is_closed, due_at | `repo: *Repository`; O2M Issue |

## Key relationships

- `OrgMember` — explicit M2M-with-role join model between User and Organization
- `fork_parent: ?Repository` — self-referential optional FK capturing fork lineage
- `Issue.is_pull + PullRequest.issue: *Issue` — 1:1 extension pattern (single-table issues + PRs)
- `PullRequest.head_repo / base_repo: *Repository` — two FKs to same model
- `Label.issues: [Issue]` + `Issue.labels: [Label]` — bidirectional M2M
- `Team.members: [User]` + `User.teams: [Team]` — bidirectional M2M
- `Star` — explicit join model (vs simple M2M) to carry `created_at`

## Grammar features showcased

- `?Repository` self-referential optional FK (`fork_parent`)
- Explicit join model with payload (`OrgMember` role, `Star` created_at)
- Two FKs to the same model in one model (`PullRequest.head_repo` + `base_repo`)
- `string @length(7, 7)` for hex color codes — text, so `string` rather than `bytes(7)`
- `bytes(20)?` on `PullRequest.merge_commit_sha` — a git object id is genuinely 20 raw bytes, which is what `bytes(N)` is for
- Bidirectional M2M auto-detection (`Issue` ↔ `Label`, `Team` ↔ `User`)
- `?Organization` optional FK on `Repository`
- Composite `@index` on FK fields (`@index(user, org)`, `@index(issue, created_at)`)
- `@default(value)` with identifier literals, `@length`, `@email`, `@url`
- `@soft_delete` not used here (see publishing-membership for that)
