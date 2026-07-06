# student-information-system

University Student Information System: departments, courses, sections per term, instructors, students, and grade-carrying enrollments.

**Domain:** Education / university SIS  
**Provenance:** Synthetic; adapted from teaching database design examples at [Creately](https://creately.com/guides/er-diagrams-for-a-university-management-system/). No license. Not a derivative work.

## Models (7)

| Model | Key fields | Relationships |
|---|---|---|
| `Department` | name (unique), code (unique+indexed), building | O2M Course, Instructor |
| `Instructor` | first_name, last_name, email (unique), title, office | `department: *Department`; O2M Section |
| `Student` | student_number (unique+indexed), first_name, last_name, email (unique), gpa (@min/@max), enrollment_status | `major: ?Department` (optional FK); O2M Enrollment |
| `Term` | name (unique), code (unique+indexed), season, year, start_date, end_date | O2M Section |
| `Course` | code (unique+indexed), title, description (@fulltext), credits (@min/@max), level | `department: *Department`; O2M Section |
| `Section` | section_number, room, schedule, max/current_enrollment, is_cancelled | `course: *Course`, `instructor: *Instructor`, `term: *Term`; O2M Enrollment; @index(course, term) |
| `Enrollment` | grade, grade_points (@min/@max), status | `student: *Student`, `section: *Section`; @index(student, section) |

## Key relationships

- `Enrollment` — the textbook M2M-with-payload (association entity) pattern: `Student` × `Section` with `grade`, `grade_points`, `status`, `enrolled_at`, `completed_at` as attributes ON the relationship
- `Section` ties together `Course`, `Instructor`, and `Term` — three FKs on one model
- `Student.major: ?Department` — optional FK expressing major declaration
- `@index(student, section)` on `Enrollment` — mirrors `@index(student_id, section_id)` intent in ForgeDB field-name terms
- `@index(course, term)` on `Section` — efficient section lookup by offering period

## Grammar features showcased

- M2M with payload via explicit join model (`Enrollment` with grade and status attributes)
- Three required FKs on a single model (`Section.course`, `.instructor`, `.term`)
- Optional FK to self as a non-identity reference (`Student.major: ?Department`)
- Composite `@index` on both FK fields of the join model (`@index(student, section)`)
- `@fulltext` on `Course.description`
- `f64` for GPA with `@min(0) @max(4)` numeric constraints
- `u32` with `@min` / `@max` for credits (0–12) and course level (100–999)
- `@min(1900) @max(2100)` on `Term.year`
- `+timestamp` auto-generated for `admitted_at` and `enrolled_at`
- `timestamp` (non-auto) for `Term.start_date` / `Term.end_date` — ForgeDB uses timestamps for all temporal values since there is no `date` type
