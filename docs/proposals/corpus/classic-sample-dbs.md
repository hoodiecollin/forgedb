# Classic SQL Sample / Example Databases — Catalog for ForgeDB Inspiration

Research corpus of well-known, freely-available sample databases used to teach SQL and
application data modeling. These are **inspiration** for hand-authored ForgeDB `.forge`
schemas — we catalog the models to reimagine them, not to copy SQL. PostgreSQL versions
are prioritized where they exist.

Symbols referenced below map to ForgeDB schema features: self-reference, M2M join tables,
composite keys, temporal/valid-time ranges, geographic hierarchies, enums, money/decimal,
reference-data lookups, mutual/cyclic FKs.

---

## The DVD-Rental Family: Sakila / Pagila / dvdrental

All three model the same domain — a **DVD rental store** (films, actors, inventory copies,
customers, rentals, payments, staff, stores). Pagila and dvdrental are PostgreSQL
ports/derivatives of the original MySQL Sakila.

### Sakila (MySQL original)

- **Domain:** Canonical MySQL sample DB — a DVD rental store, built to showcase MySQL
  features (views, stored procedures, triggers).
- **Canonical source / license:**
  - Docs: https://dev.mysql.com/doc/sakila/en/
  - License: https://dev.mysql.com/doc/sakila/en/sakila-license.html — the
    `sakila-schema.sql` and `sakila-data.sql` files are under the **New BSD license**
    (surrounding docs are not open-licensed).
- **Engines:** MySQL (upstream schema many other engines port from).
- **Distribution:** `sakila-schema.sql` (DDL + views + stored procs + triggers) and
  `sakila-data.sql` (INSERTs).
- **Tables (16):** `actor`, `film`, `film_actor` (M2M), `film_category` (M2M), `category`,
  `film_text` (denormalized fulltext for MyISAM), `language`, `inventory`, `rental`,
  `payment`, `customer`, `staff`, `store`, `address`, `city`, `country`.
- **Key relationships:**
  - **M2M:** `film_actor` (film↔actor), `film_category` (film↔category).
  - **Double self-ish FK:** `film.language_id` and `film.original_language_id` both →
    `language` (the nullable second reference is a nice modeling wrinkle).
  - **Geographic hierarchy:** `country` ← `city` ← `address` ← {`customer`, `staff`, `store`}.
  - **Inventory→rental→payment chain:** `inventory`(film,store) → `rental`(inventory,
    customer, staff) → `payment`(rental, customer, staff).
  - **Mutual/cyclic FK:** `store.manager_staff_id` → `staff` while `staff.store_id` → `store`.
  - Views: `customer_list`, `film_list`, `sales_by_store`, `sales_by_film_category`,
    `actor_info`, etc.
- **Notable modeling features:** two M2M joins; geographic drill-down; mutual FK cycle;
  temporal rental data (rental_date/return_date); DECIMAL money (`payment.amount`,
  `film.rental_rate`, `film.replacement_cost`); **ENUM** `film.rating` (G/PG/PG-13/R/NC-17);
  **SET** `film.special_features`; fulltext via `film_text`.
- **ForgeDB fit:** The single most iconic sample DB — media-rental slot. Rich enough to
  showcase M2M, enums, money, temporal, and hierarchies in one schema.

### Pagila (PostgreSQL port of Sakila)

- **Domain:** Faithful PostgreSQL port of Sakila — same DVD-rental domain, idiomatic Postgres.
- **Canonical source / license:**
  - Repo: https://github.com/devrimgunduz/pagila
  - Schema: https://github.com/devrimgunduz/pagila/blob/master/pagila-schema.sql
  - License: **PostgreSQL License** (permissive, BSD-style) — note this differs from
    Sakila's New BSD labeling.
- **Engine:** PostgreSQL 12+.
- **Distribution:** `pagila-schema.sql`, `pagila-data.sql` (COPY), `pagila-insert-data.sql`
  (INSERT), `pagila-schema-jsonb.backup` (pg_restore); Docker/Compose included.
- **Tables:** Same core entity set and FK web as Sakila; **no `film_text`** (replaced by
  native Postgres fulltext).
- **Notable features / differences vs Sakila:**
  - **Native fulltext:** `film.fulltext` `tsvector` column maintained by a trigger.
  - **Partitioned `payment`:** declaratively range-partitioned by date (monthly partitions).
  - Real `boolean` columns (vs MySQL `char(1)` flags); native enum/domain types for
    `rating`/`special_features` (Postgres uses `text[]`/enum, not MySQL SET).
  - `last_update` columns maintained by triggers; **JSONB** sample data added in v3.0.0.
- **ForgeDB fit:** The Postgres-native reference variant — best when we want to showcase
  fulltext + partitioning + JSONB alongside the DVD-rental model.

### dvdrental (PostgreSQL Tutorial sample DB)

- **Domain:** The PostgreSQL Tutorial teaching DB — same DVD-rental domain, "created from
  MySQL's Sakila" and repackaged as a Postgres dump.
- **Canonical source / license:**
  - https://www.postgresqltutorial.com/postgresql-getting-started/postgresql-sample-database/
    (now redirects to https://neon.com/postgresql/postgresql-getting-started/postgresql-sample-database)
  - No distinct OSS license stated — a free downloadable teaching artifact derived from Sakila.
- **Engine:** PostgreSQL only.
- **Distribution:** binary `pg_restore` archive (`dvdrental.zip` → `dvdrental.tar`), not
  plain SQL — the most "just load and go" of the three.
- **Object counts:** 15 tables, 1 trigger, 7 views, 8 functions, 1 domain, 13 sequences.
- **Tables (15):** Same as Sakila minus `film_text`. Same FK web and two M2M joins.
- **Notable features / differences:** vanilla Postgres port (no partitioning, no tsvector);
  uses a Postgres **domain** and 13 sequences; `rating` uses `mpaa_rating` enum,
  `special_features` is `text[]`.
- **ForgeDB fit:** The plainest DVD-rental Postgres variant — good baseline reference.

**Family comparison:**

| Aspect | Sakila | Pagila | dvdrental |
|---|---|---|---|
| Engine | MySQL (upstream) | PostgreSQL 12+ | PostgreSQL |
| License | New BSD | PostgreSQL License | free teaching artifact (from Sakila) |
| Tables | 16 (incl. `film_text`) | ~15 (no `film_text`) | 15 (no `film_text`) |
| Fulltext | `film_text` (MyISAM) | native `tsvector` + trigger | none |
| `payment` | single | range-partitioned (monthly) | single |
| Booleans | `char(1)` | real `boolean` | boolean |
| JSONB | no | yes (v3.0.0) | no |
| Distribution | 2 `.sql` files | `.sql` + jsonb backup + Docker | binary pg_restore `.tar` |

---

## Chinook (digital media store)

- **Domain:** iTunes-like digital media store — artists, albums, tracks, playlists, plus the
  sales side (customers, invoices, support-rep employees).
- **Canonical source / license:**
  - Repo: https://github.com/lerocha/chinook-database
  - License: **MIT** (verified) — fully open, safe to vendor directly.
- **Engines:** Ships ready-made SQL for **six** engines: DB2, MySQL, Oracle, **PostgreSQL**,
  SQL Server, SQLite. Postgres is a first-class official target.
- **Tables (11) + relationships:**
  - `Artist` → `Album` (FK ArtistId) → `Track` (FK AlbumId, GenreId, MediaTypeId)
  - `Genre`, `MediaType` (lookups)
  - `Playlist` — `PlaylistTrack` (**M2M**, composite PK PlaylistId+TrackId) — `Track`
  - `Customer` (FK SupportRepId → Employee)
  - `Employee` (**self-ref** FK ReportsTo → Employee — manager hierarchy)
  - `Invoice` (FK CustomerId) → `InvoiceLine` (FK InvoiceId, TrackId — line items)
- **Notable modeling features:** self-referencing employee hierarchy; M2M `PlaylistTrack`;
  DECIMAL money (`Track.UnitPrice`, `InvoiceLine.UnitPrice`, `Invoice.Total`); invoice
  line-item pattern; long FK chain (Artist→Album→Track→InvoiceLine→Invoice→Customer→Employee);
  geographic/address fields on Customer/Employee/Invoice.
- **ForgeDB fit:** Excellent clean-but-rich media/e-commerce example — "consumer app /
  digital storefront" slot. MIT license makes it safe to adapt; slightly more approachable
  than Northwind for FK chains + M2M.

---

## Northwind (orders / trading company)

- **Domain:** Microsoft's classic wholesale trading company — customers order products from
  suppliers, fulfilled by employees and shippers. Lineage: Pubs → **Northwind** →
  AdventureWorks → WideWorldImporters.
- **Canonical source / license:**
  - Original: Microsoft SQL Server / Access sample (`instnwnd.sql`), MS sample terms.
  - Official repo: https://github.com/microsoft/sql-server-samples/tree/master/samples/databases/northwind-pubs
  - Popular Postgres port: https://github.com/pthom/northwind_psql (single `northwind.sql`
    + Docker Compose; port carries its own LICENSE — verify before shipping).
- **Engines:** Originally SQL Server + Access; **PostgreSQL** via well-established community
  ports (pthom being the most popular); MySQL/SQLite ports also exist.
- **Tables (~13) + relationships:**
  - `Categories`, `Suppliers` → `Products` (FK SupplierID, CategoryID)
  - `Customers` (char(5) natural key)
  - `Employees` (**self-ref** ReportsTo)
  - `Shippers`
  - `Orders` (FK CustomerID, EmployeeID, ShipVia→Shippers)
  - `Order Details` / `OrderDetails` (**M2M** order↔product, composite PK, carries
    UnitPrice/Quantity/Discount — association-with-attributes)
  - `Region` → `Territories` (FK RegionID)
  - `EmployeeTerritories` (**M2M** employee↔territory, composite PK)
  - `CustomerDemographics` — `CustomerCustomerDemo` (**M2M**) — `Customers`
- **Notable modeling features:** self-ref hierarchy; **three** M2M joins; join-with-payload
  (`OrderDetails`); DECIMAL money (`UnitPrice`, `Freight`); composite keys throughout;
  extensive geo/address fields + Region/Territories hierarchy; non-integer natural key
  (`Customers.CustomerID` is a 5-char code). Caveat: the `Order Details` table name has an
  embedded space — normalize in a `.forge` schema.
- **ForgeDB fit:** Strong "enterprise B2B / ERP-style orders" example — business/logistics
  slot; stresses composite keys, join-with-payload, and geo hierarchies harder than Chinook.

---

## world (geographic reference data)

- **Domain:** Countries, their cities, and languages spoken — the canonical MySQL/Postgres
  "world" sample.
- **Canonical source / license:**
  - MySQL: https://dev.mysql.com/doc/world-setup/en/ (`world.sql` from
    https://dev.mysql.com/doc/index-other.html). Data originally from Statistics Finland;
    freely redistributable. `world_x` variant adds a JSON `countryinfo` table.
  - Postgres ports: https://www.postgresql.org/ftp/projects/pgFoundry/dbsamples/world/ and
    Docker image https://hub.docker.com/r/ghusta/postgres-world-db (v2.0+ uses snake_case
    `country_language`).
- **Engines:** MySQL (official), PostgreSQL (community port, explicitly available).
- **Tables (3) + relationships:**
  - `country` — PK `Code` CHAR(3); columns Continent, Region, Population, LifeExpectancy,
    GNP, Capital. `Capital` is FK → `city.ID`.
  - `city` — PK ID; FK `CountryCode` → country. (~4,079 rows)
  - `countrylanguage` — **composite PK (CountryCode, Language)**; FK → country; IsOfficial,
    Percentage. (M2M-style bridge)
  - Volume: 239 countries, 4,079 cities. Note **cyclic FK** country↔city (capital).
- **Notable modeling features:** ENUM fields (`Continent`, `Region`); composite PK; cyclic
  FK; small clean reference dataset.
- **ForgeDB fit:** Ideal small reference-data/geo example — enums, composite keys, lookup
  relationships. Domain slot: geographic reference data.

---

## classicmodels (scale-model car retailer)

- **Domain:** Retailer of scale-model classic cars — offices, employees, customers, orders,
  products, payments (classic small-business ERP/CRM).
- **Canonical source / license:**
  - MySQL Tutorial "MySQL Sample Database":
    https://www.mysqltutorial.org/getting-started-with-mysql/mysql-sample-database/
    (originally from Richard T. Watson). Widely mirrored (Azure-Samples/mysql-database-samples).
    Free educational use; no formal license.
  - Postgres port: https://github.com/LarsOevlisen/classicmodels_postgresql_14 (community).
- **Engines:** MySQL/InnoDB (original, enforced FKs), PostgreSQL (community port).
- **Tables (8) + relationships:**
  - `offices` (PK officeCode)
  - `employees` (**self-ref** reportsTo → employees; FK officeCode → offices; jobTitle)
  - `customers` (FK salesRepEmployeeNumber → employees; creditLimit)
  - `productlines` → `products` (FK productLine; buyPrice, MSRP)
  - `orders` (FK customerNumber; status, orderDate, requiredDate, shippedDate)
  - `orderdetails` (**composite PK (orderNumber, productCode)** = M2M orders↔products;
    quantityOrdered, priceEach)
  - `payments` (**composite PK (customerNumber, checkNumber)**; amount, paymentDate)
- **Notable modeling features:** self-referential org hierarchy; M2M-with-payload
  (orderdetails); two composite PKs; money fields (creditLimit, prices, payments);
  realistic normalized business schema; order-status field (soft state machine).
- **ForgeDB fit:** The strongest "business app / retail-ERP" showcase — self-ref FK, M2M
  with attributes, money, and multi-level relationships in one moderate schema. Domain
  slot: e-commerce / retail business.

---

## employees / test_db (HR with temporal history)

- **Domain:** Large synthetic HR dataset with full temporal salary/title/assignment history
  — the canonical slowly-changing-dimension / valid-time example.
- **Canonical source / license:**
  - Repo: https://github.com/datacharmer/test_db
    (schema: https://github.com/datacharmer/test_db/blob/master/employees.sql)
  - License: **Creative Commons Attribution-Share Alike 3.0 Unported**. Data fabricated.
  - Ships MySQL variants (standard + `employees_partitioned.sql` + validation scripts).
    Postgres via community ports (e.g. bundled in morenoh149/postgresDBSamples).
- **Engines:** MySQL (original, InnoDB with cascading-delete FKs); PostgreSQL via ports.
- **Tables (6 + 2 views) + relationships:**
  - `employees` (PK emp_no; birth_date, gender, hire_date). ~300,024 rows.
  - `departments` (PK dept_no CHAR(4); UNIQUE dept_name). 9 rows.
  - `dept_emp` (**composite PK (emp_no, dept_no)**; FKs both; **temporal from_date/to_date**).
    ~331,603 rows.
  - `dept_manager` (**composite PK**; temporal from_date/to_date). 24 rows.
  - `salaries` (**composite PK (emp_no, from_date)**; **valid-time from_date/to_date**).
    ~2,844,047 rows.
  - `titles` (**composite PK (emp_no, title, from_date)**; from_date/nullable to_date).
    ~443,308 rows.
  - Two views track current department assignments; history tables cascade-delete to employees.
- **Notable modeling features:** THE temporal / slowly-changing-dimension exemplar — salary
  and title history as `(from_date, to_date)` valid-time ranges in composite PKs; temporal
  M2M (dept_emp, dept_manager); large volume (~3.9M+ rows) for scale testing.
- **ForgeDB fit:** Best showcase for temporal data / append-only history and scale — valid-
  time ranges, composite temporal keys, big-volume codegen. Domain slot: HR / workforce
  management with history.

---

## Microsoft AdventureWorks (enterprise bicycle manufacturer)

- **Domain:** OLTP database for fictional bicycle manufacturer "Adventure Works Cycles" —
  sales, production, purchasing, HR — plus companion star-schema DW (AdventureWorksDW). The
  canonical *large enterprise* teaching database.
- **Canonical source / license:**
  - Official: https://learn.microsoft.com/en-us/sql/samples/adventureworks-install-configure
    and https://github.com/microsoft/sql-server-samples/tree/master/samples/databases/adventure-works
    (MIT-licensed samples repo). Distributed as `.bak` restore files (OLTP, DW, LT variants).
  - **Postgres port:** https://github.com/lorint/AdventureWorks-for-Postgres (Ruby CSV
    converter + `install.sql`; builds all 68 tables + 11/20 views, converts `hierarchyid`).
    Also https://github.com/NorfolkDataSci/adventure-works-postgres.
- **Engines:** SQL Server (native, 2008→2012+, Azure SQL, Fabric); PostgreSQL (community port).
- **Schema groups (~70 tables / 5 schemas):**
  - **Person** — Person, BusinessEntity, EmailAddress, Address, PhoneNumber, CountryRegion,
    StateProvince (identity hub).
  - **HumanResources** — Employee, Department, Shift, EmployeeDepartmentHistory,
    EmployeePayHistory (temporal employment history).
  - **Production** — Product, ProductCategory, ProductSubcategory, ProductModel,
    BillOfMaterials (**self-ref M2M product assembly**), WorkOrder, Inventory.
  - **Purchasing** — Vendor, PurchaseOrderHeader, PurchaseOrderDetail, ProductVendor.
  - **Sales** — SalesOrderHeader, SalesOrderDetail, Customer, SalesPerson, SalesTerritory,
    Store, SpecialOffer.
- **Marquee relationships:** `SalesOrderHeader → SalesOrderDetail → Product`;
  `Employee` → `Person.BusinessEntityID`; `BillOfMaterials` self-referencing assembly graph;
  temporal `EmployeeDepartmentHistory`/`EmployeePayHistory`.
- **Notable modeling features:** very large multi-schema model; `hierarchyid` org chart &
  category trees; temporal history tables; AdventureWorksDW recasts as a Kimball star schema
  (FactInternetSales, FactResellerSales + DimCustomer/DimProduct/DimDate/DimEmployee) — the
  go-to OLTP↔DW contrast example.
- **ForgeDB fit:** The stress-test / enterprise fixture — many schemas, self-refs, temporal
  history, deep FKs at once. Domain slot: commerce / manufacturing ERP. Heavier than
  ForgeDB's demo sweet spot; better for scale testing than a first showcase.

---

## Oracle HR sample schema (org directory)

- **Domain:** Oracle's classic "Human Resources" schema — a compact org-chart + geographic
  hierarchy used across virtually every Oracle SQL tutorial.
- **Canonical source / license:**
  - Docs: https://docs.oracle.com/en/database/oracle/oracle-database/19/comsc/HR-sample-schema-table-descriptions.html
  - Install scripts: https://github.com/oracle-samples/db-sample-schemas/blob/main/human_resources/hr_create.sql
    License: **UPL (Universal Permissive License)**.
- **Engines:** Oracle Database (native). Small/standard DDL → trivially ported; many
  community MySQL/Postgres re-implementations but no single "official" Postgres port.
- **Tables (7) + relationships:**
  - `REGIONS` (PK region_id). 5 rows.
  - `COUNTRIES` (PK country_id CHAR(2); FK region_id → REGIONS). 25 rows.
  - `LOCATIONS` (PK location_id; FK country_id → COUNTRIES). 23 rows.
  - `DEPARTMENTS` (PK department_id; FK manager_id → EMPLOYEES; FK location_id → LOCATIONS). 27.
  - `EMPLOYEES` (PK employee_id; **self-ref manager_id**; FK department_id; FK job_id;
    salary, commission_pct). 107 rows.
  - `JOBS` (PK job_id; min_salary, max_salary). 19 rows.
  - `JOB_HISTORY` (**composite PK (employee_id, start_date)**; end_date; FKs job_id,
    department_id, employee_id — temporal). 10 rows.
  - Note **mutual FK cycle:** DEPARTMENTS.manager_id → EMPLOYEES while
    EMPLOYEES.department_id → DEPARTMENTS.
- **Notable modeling features:** textbook self-referencing org chart; clean geographic
  hierarchy region→country→location→department; temporal JOB_HISTORY with composite PK;
  mutual FK cycle.
- **ForgeDB fit:** The ideal small, expressive demo schema — self-refs, a linear hierarchy,
  and a composite-key temporal table in ~100 rows. Domain slot: HR / org directory. Best
  first showcase of ForgeDB's relation symbols.

---

## LEGO / Rebrickable database (catalog / bill-of-materials)

- **Domain:** Rebrickable's catalog of every official LEGO set — sets, parts, colors,
  inventories, themes — popularized via a Kaggle export; used in many SQL/modeling courses.
- **Canonical source / license:**
  - Live CSV downloads (daily): https://rebrickable.com/downloads/ ; schema docs
    https://rebrickable.com/help/lego-database/. Free for non-commercial use (attribution).
  - Frozen teaching snapshot: https://www.kaggle.com/datasets/rtatman/lego-database
    (8 tables, 633,250 rows, 11,673 sets 1950–2017). Updated 2025 export:
    https://www.kaggle.com/datasets/iamjcmc/lego-database-2025. Teaching writeup:
    https://files.eric.ed.gov/fulltext/EJ1468081.pdf
- **Engines:** engine-agnostic CSV → loaded into SQLite / Postgres / MySQL in tutorials. No
  single canonical engine.
- **Tables + the M2M web:**
  - `themes` (PK id; **self-ref parent_id** — theme tree, e.g. Star Wars → Episode I)
  - `sets` (PK set_num; FK theme_id; num_parts)
  - `part_categories` → `parts` (PK part_num; FK part_cat_id)
  - `colors` (PK id; rgb, is_trans)
  - `inventories` (PK id; version; FK set_num → sets) — a set can have multiple versions
  - `inventory_parts` (FK inventory_id, part_num, color_id; quantity, is_spare) — the
    **junction realizing set⇄part M2M** (part-in-a-color-in-an-inventory)
  - `inventory_sets` (FK inventory_id, set_num; quantity) — **set⇄set** boxed bundles
  - `elements` (PK element_id; FK part_num, color_id) — official element IDs (part+color)
  - Newer live schema adds `minifigs`, `inventory_minifigs`, `part_relationships`.
  - **Reconstructed M2M path:** `sets → inventories → inventory_parts → parts` (a set's bill
    of materials), with color as a third dimension — a three-hop M2M mediated by inventories,
    not a direct join.
- **Notable modeling features:** deep/indirect M2M; self-referencing theme hierarchy; rich
  shared reference/lookup tables (colors, part_categories); versioned inventories
  (temporal-ish variants).
- **ForgeDB fit:** Best M2M + reference-data showcase — join tables, self-ref hierarchy, and
  lookup enums together at a fun, tangible scale. Domain slot: catalog / bill-of-materials.

---

## Other notable sample DBs (substantiated)

- **Microsoft WideWorldImporters** — modern successor to Northwind/AdventureWorks (SQL
  Server 2016+); fictional novelty-goods wholesaler; OLTP + DW variants.
  https://github.com/microsoft/sql-server-samples . Enterprise/DW stress fixture.
- **Pubs** — the original tiny book-publishing sample (authors/titles/publishers); ancestor
  of Northwind. https://github.com/microsoft/sql-server-samples/tree/master/samples/databases/northwind-pubs
- **Lahman Baseball Database** — MLB batting/pitching/fielding stats 1871–2024; `People`,
  `Batting`, `Pitching`, `Teams`, `Salaries`. CC BY-SA 3.0. http://seanlahman.com/download-baseball-database/ ;
  SQLite port https://github.com/jknecht/baseball-archive-sqlite . Denormalized analytics fixture.
- **Stack Exchange / Stack Overflow data dump** — anonymized dump (`Posts`, `Users`, `Votes`,
  `Comments`, `Badges`, `Tags`, `PostHistory`, `PostLinks`), CC BY-SA. Full dump on
  https://archive.org/details/stackexchange ; sized SQL Server copies
  https://github.com/BrentOzarULTD/Stack-Overflow-Database . Big social-graph fixture.
- **MusicBrainz** — open music metadata; `artist`, `release`, `release_group`, `recording`,
  `work`, `label`, `area`, `place`, plus `*_tag` tables. Postgres dumps.
  https://musicbrainz.org/doc/MusicBrainz_Database/Schema . Very large, heavily normalized.
- **OpenFlights** — airports/airlines/routes/planes CSV under ODbL; ~67,663 routes / 3,321
  airports / 548 airlines. route = airline × src × dst (clean graph M2M).
  https://openflights.org/data.php
- **TPC-H** — decision-support benchmark, 3NF, 8 tables (`region, nation, supplier, part,
  partsupp, customer, orders, lineitem`) + 22 queries.
  https://docs.starburst.io/starburst-galaxy/working-with-data/create-catalogs/sample-data-sets/tpch.html
- **TPC-DS** — retail decision-support benchmark, snowflake of ~24 tables (7 fact + 17
  dimension). https://clickhouse.com/docs/getting-started/example-datasets/tpcds . Canonical
  DW/star-schema fixture.
- **Elmasri & Navathe COMPANY** — textbook schema: `EMPLOYEE, DEPARTMENT, DEPT_LOCATIONS,
  PROJECT, WORKS_ON, DEPENDENT`; self-ref supervisor; `WORKS_ON` = employee⇄project M2M.
  https://github.com/tolgahanakgun/Elmasri-Database . Small self-ref + M2M — close to Oracle HR.
- **University / enrollment schema** — generic course-catalog model (`Student, Course,
  Enrollment, Faculty`) with student⇄class M2M via enrollment; many academic variants.
  https://dsf.berkeley.edu/topics/lecs/dbprimer/tsld008.htm
- **IMDB / TMDB** — widely used in courses but IMDB's official non-commercial datasets are
  flat TSVs (https://developer.imdb.com/non-commercial-datasets/), not a normalized schema;
  TMDB (Kaggle/API, CC) is the usual normalized source. Less "canonical schema" than others.
- **Spotify datasets** — Kaggle audio-features CSVs are single-table extracts, not relational
  sample DBs; excluded as a real multi-table sample.

---

## Domain-slot coverage summary

| Domain slot | Best candidates |
|---|---|
| Media / entertainment storefront | Sakila/Pagila/dvdrental, Chinook |
| E-commerce / retail ERP | classicmodels, Northwind |
| Enterprise / manufacturing ERP + DW | AdventureWorks, WideWorldImporters, TPC-DS |
| HR / org directory | Oracle HR, Elmasri COMPANY |
| HR with temporal history | employees/test_db, AdventureWorks HR |
| Geographic reference data | world, OpenFlights |
| Catalog / bill-of-materials | LEGO/Rebrickable, TPC-H |
| Analytics / stats | Lahman Baseball, Stack Overflow, MusicBrainz |
| Academic / course enrollment | University schema, Elmasri COMPANY |

## Top picks for ForgeDB adaptation

1. **Sakila / Pagila** (media rental) — the iconic sample; one schema exercises M2M, enums,
   money, temporal rentals, geo hierarchy, and mutual FKs. Prefer the Pagila Postgres form.
2. **Chinook** (digital media store) — MIT-licensed, clean FK chains + self-ref + M2M +
   money; approachable, Postgres-first, safe to adapt.
3. **Oracle HR** (org directory) — the ideal *small* first demo: self-ref org chart,
   geographic hierarchy, composite-key temporal JOB_HISTORY, ~100 rows.
4. **classicmodels** (retail ERP) — moderate business schema with self-ref, M2M-with-payload,
   composite keys, money, and an order-status state field; Postgres port exists.
5. **LEGO / Rebrickable** (catalog / BOM) — best deep-M2M + self-ref-hierarchy + reference-
   data showcase, at a fun tangible scale; engine-agnostic CSV.
6. **employees / test_db** (temporal HR) — the definitive temporal / valid-time / slowly-
   changing-dimension exemplar and a scale fixture; fills the "history + volume" slot no
   other pick covers.
