# blog-cms

A canonical blog/CMS platform.

**Domain:** Content management — publishing, taxonomy, community comments.

**Provenance:** Synthetic (invented from data-modeling knowledge).

---

## Models and key relationships

| Model | Key fields | Relations |
|-------|-----------|-----------|
| `User` | `email &string`, `username ^&string` | has many `Post`, `Comment`; component refs |
| `Post` | `slug ^&string`, `status`, `content @fulltext` | `*User` author, `?Category`, many `Comment`, M2M `Tag`; `@soft_delete` |
| `Comment` | `body`, `created_at` | `*User` author, `*Post`, self-ref `?Comment` parent |
| `Category` | `slug ^&string` | self-ref `?Category` parent, many `Post` |
| `Tag` | `name ^&string`, `slug ^&string` | M2M `Post` (bidirectional `[Post]`/`[Tag]`) |

**Relation summary:**
- `Post.tags: [Tag]` + `Tag.posts: [Post]` — parser auto-detects M2M
- `Comment.parent: ?Comment` — self-referential thread nesting
- `Category.parent: ?Category` — self-referential category tree
- `Post.category: ?Category` — optional FK (nullable relation)

---

## Grammar features showcased

- `@fulltext` on `Post.content`
- `@soft_delete` model directive on `Post`
- Self-referential optional FK (`?Comment`, `?Category`)
- Bidirectional M2M via `[Tag]`/`[Post]` (no explicit join model needed)
- Optional FK (`?Category`) alongside required FK (`*User`)
- Composite `@index(status, created_at)` on `Post`
- **Correct snake_case component-reference field names** (the whole point of this example):
  - `profile_card: tsx://components/user/ProfileCard @relations(posts, comments)` in `User`
  - `post_editor: jsx://components/post/Editor @relations(author, category)` in `Post`
  - `publish_endpoint: api://routes/post/publish` in `Post`
  - `update_endpoint: api://routes/user/update` in `User`
- `@email`, `@url` constraints on `User` fields
- `@length` constraints throughout
- `+timestamp` auto-generate, `timestamp?` nullable updated_at
- `string?` nullable optional fields (`bio`, `excerpt`, `color`)

---

## Grammar limitation noted

The `.forge` lexer does not tokenize quoted string literals, so `@pattern("regex")` and `@default("value")` are not valid. Status enums are represented as `string` with `@default(draft)` (unquoted identifier) and documented in comments rather than enforced with regex patterns.
