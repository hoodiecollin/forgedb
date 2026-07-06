# hr-directory

A compact HR org-chart and geographic hierarchy — the introductory ForgeDB example.

**Domain:** Human resources / organizational directory

**Provenance:** Adapted from Oracle HR sample schema
(https://github.com/oracle-samples/db-sample-schemas, UPL — Universal Permissive License)

---

## Models (7)

| Model | Description |
|---|---|
| `Region` | Top of the geographic hierarchy (e.g. Americas, Europe) |
| `Country` | Country within a region; natural key `country_code` char(2) |
| `Location` | Physical site within a country (street address, city) |
| `Job` | Job title with salary band (`min_salary`/`max_salary` as i64 cents) |
| `Department` | Org unit at a location; optional `manager` FK to Employee |
| `Employee` | Staff member; self-referencing `manager: ?Employee`; `salary` as i64 cents |
| `JobHistory` | Temporal record of an employee's past assignments (start/end timestamps) |

## Key Relationships

- **Geographic hierarchy:** Region → Country → Location → Department (one-to-many chain)
- **Self-referencing org chart:** `Employee.manager: ?Employee` with reverse collection `Employee.subordinates: [Employee]`
- **Mutual FK cycle:** `Department.manager: ?Employee` and `Employee.department: ?Department` — both optional to avoid insertion-order deadlock (mirrors Oracle HR's nullable FK pattern)
- **Temporal assignments:** `JobHistory` records past (employee, job, department) with `start_date`/`end_date` timestamps; composite index `@index(employee, start_date)` captures the common temporal query pattern (replaces Oracle HR's composite PK)

## Grammar Features Showcased

- `?Model` optional FK (Employee.manager, Employee.department, Department.manager)
- `*Model` required FK (Country.region, Location.country, Employee.job)
- `[Model]` one-to-many reverse collections (Region.countries, Department.employees, Employee.subordinates)
- Self-referencing one-to-many via optional FK (`manager`/`subordinates` on same model)
- Mutual FK cycle between two models (Department ↔ Employee)
- `+uuid` primary keys, `+timestamp` created_at, `timestamp?` updated_at/end_date
- `&` unique on natural key (`country_code`, `job_title`)
- `i64` for money (salary ranges)
- `@min`/`@max` on numeric fields, `@length` on string fields, `@email` on contact fields
- Model-level composite index `@index(employee, start_date)` on JobHistory

## Grammar Limitation Noted

- Oracle HR's composite PK `(employee_id, start_date)` on `JOB_HISTORY` is modeled as a `+uuid` PK with `@index(employee, start_date)` — ForgeDB does not support composite primary keys.
