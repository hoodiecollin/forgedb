# Tutorial, Doc &amp; ORM Sample Schemas — ForgeDB Example Inspiration Catalog

Research corpus of SAMPLE database schemas from (a) programming tutorials/courses and
(b) official database &amp; ORM documentation, gathered as **inspiration** for hand-authored
ForgeDB `.forge` example schemas. ForgeDB is a compile-time DB generator with its own
schema language — this catalog captures the **data model (entities + relationships +
patterns) to reimagine**, NOT DDL/code to copy.

**Licensing meta-note:** Almost none of these are re-distributable "just copy it" assets,
and several teaching articles carry no license at all. The intended use is one-directional:
read the domain, understand the entities, and author fresh `.forge` schemas. Where a real
open license exists (BSD Sakila/Pagila, MIT Northwind/TypeORM, Apache Prisma/Drizzle/Ecto/
Supabase, CC-BY-SA Rails/Django-Girls/Employees) it is noted per item.

Sections:
- **Part A** — Classic SQL/Postgres teaching sample databases
- **Part B** — Official ORM / framework example schemas
- **Part C** — Teaching-site "database design example" schemas (domain-gap fillers)
- **Part D** — MongoDB / NoSQL sample datasets (document-model contrast)
- **Part E** — Coverage matrix, gap analysis &amp; top picks

---

# Part A — Classic SQL / Postgres Teaching Sample Databases

## A1. DVD Rental (PostgreSQL Tutorial / Pagila)

- **Domain:** Film/DVD rental store — the canonical PostgreSQL teaching DB.
- **Source:** https://neon.com/postgresql/postgresql-getting-started/postgresql-sample-database · mirror https://github.com/neondatabase/postgres-sample-dbs
- **Source type / license:** Tutorial sample DB (postgresqltutorial.com / Neon). A Postgres port of Sakila → inherits **New BSD**. 15 tables.
- **Core entities &amp; relationships:**
  - `film` *—* `actor` via **`film_actor`**; `film` *—* `category` via **`film_category`** (two M:N junctions).
  - `film` → `language`; `inventory` → `film` + `store` (physical copies).
  - `rental` → `inventory` + `customer` + `staff`; `payment` → `rental` + `customer` + `staff`.
  - `customer`/`staff`/`store` → `address` → `city` → `country` (3-level location chain).
- **Notable patterns:** two M:N junctions, address→city→country hierarchy, abstract-film vs physical-inventory-copy indirection, payments tied to rentals, staff/manager cross-references.
- **ForgeDB value:** The go-to "medium-complexity relational" slot — rich joins/M:N/aggregation without being overwhelming.

## A2. Sakila (MySQL) — the original

- **Domain:** Film/DVD rental store; the origin that DVD-Rental/Pagila derive from.
- **Source:** https://dev.mysql.com/doc/sakila/en/ · license https://dev.mysql.com/doc/sakila/en/sakila-license.html
- **Source type / license:** Official MySQL docs sample; schema+data are **New BSD licensed**.
- **Core entities:** Same shape as A1 (film, actor, category, inventory, rental, payment, customer, staff, store, address/city/country) **plus views, stored procedures, triggers, and a `film_text` full-text table** the plain Postgres port omits.
- **Notable patterns:** everything in A1 plus DB-side logic (triggers maintaining `last_update`, stored procs, views) and full-text — useful if showcasing generated triggers/derived columns.
- **ForgeDB value:** The "reference original" — cite as lineage and for full-text + trigger patterns.

## A3. Northwind

- **Domain:** Small-business trading company (specialty foods import/export) — order-management ERP.
- **Source:** https://github.com/microsoft/sql-server-samples/tree/master/samples/databases/northwind-pubs · ports https://github.com/jpwhite3/northwind-SQLite3
- **Source type / license:** Microsoft sample; sql-server-samples repo is **MIT**. ~14 tables.
- **Core entities &amp; relationships:**
  - `orders` → `customers`, `employees`, `shippers`; **`order_details`** (junction w/ unit_price/quantity/discount) → `orders` + `products` (M:N with line attributes).
  - `products` → `suppliers` + `categories`.
  - **`employees` → `employees` (self-FK `reports_to`)** — supervisor hierarchy.
  - `employees` *—* `territories` via `employee_territories`; `territories` → `region`.
- **Notable patterns:** self-referencing manager hierarchy, M:N order-line junction carrying line attributes, supplier→category→product taxonomy, shipping metadata.
- **ForgeDB value:** The "business ERP / order-management" slot — default for teaching order systems and reporting.

## A4. Chinook

- **Domain:** Digital media store (iTunes-like) — artists/albums/tracks + customer invoices. Multi-engine.
- **Source:** https://github.com/lerocha/chinook-database · https://www.sqlitetutorial.net/sqlite-sample-database/
- **Source type / license:** Community sample (SQL Server/Oracle/MySQL/Postgres/SQLite/DB2); permissive LICENSE.md. 11 core tables.
- **Core entities &amp; relationships:**
  - `artist` 1—* `album` 1—* `track`; `track` → `genre` + `media_type`.
  - `playlist` *—* `track` via **`playlist_track`**.
  - `customer` → `support_rep` (`employees`); **`employee` → `employee` (self-FK)** supervisor chain.
  - `invoice` → `customer`; `invoice_line` → `invoice` + `track`.
- **Notable patterns:** clean 3-level catalog (artist→album→track), M:N playlist junction, self-referencing employee hierarchy, invoice/invoice-line billing, denormalized billing address on invoice.
- **ForgeDB value:** The "media catalog + billing" slot — nested one-to-many catalogs, music/streaming domain distinct from film rental.

## A5. AdventureWorks (Microsoft)

- **Domain:** Multinational bicycle manufacturer/retailer OLTP — sales, production, HR, purchasing.
- **Source:** https://learn.microsoft.com/en-us/sql/samples/adventureworks-install-configure · https://dbdocs.io/blog/posts/adventure-works-database-schema/
- **Source type / license:** Official Microsoft sample; sql-server-samples repo **MIT**. ~70 tables across 5 schemas (`Person`, `HumanResources`, `Production`, `Purchasing`, `Sales`).
- **Core entities (high level):**
  - **`BusinessEntity` supertype** links `Person`, `Employee`, `Customer`, `Vendor` (shared-key supertype/subtype).
  - `Production`: products, bill-of-materials, work orders, inventory. `Sales`: order header/detail, territories, sales persons. `Purchasing`: vendors, POs. `HumanResources`: employees, departments, pay history.
- **Notable patterns:** enterprise multi-schema separation, **supertype/subtype via `BusinessEntity`**, bill-of-materials (self-referential parts explosion), temporal history tables, header/detail split.
- **ForgeDB value:** "Enterprise / advanced" reference — cite for schema namespacing + supertype; too heavy to reimplement fully, use for organizational inspiration.

## A6. Employees ("test_db", MySQL)

- **Domain:** Corporate HR — employees, departments, salaries, titles; ~4M rows.
- **Source:** https://github.com/datacharmer/test_db · https://dev.mysql.com/doc/employee/en/
- **Source type / license:** Community/MySQL sample; **CC-BY-SA 3.0**. 6 tables.
- **Core entities &amp; relationships:**
  - `employees` (300k); `departments` (9).
  - **`dept_emp`** (M:N employee↔department, **with `from_date`/`to_date`**); **`dept_manager`** (date-ranged).
  - **`salaries`** and **`titles`** — temporal fact tables keyed by employee + `from_date`, tracking history (2.8M salary rows).
- **Notable patterns:** **temporal / date-range validity** (SCD-style history), M:N with effective dates, large-volume data for perf/indexing demos.
- **ForgeDB value:** The "temporal history + scale" slot — date-range validity, versioned facts, index/aggregation perf.

## A7. Extra Postgres teaching samples (Neon collection)

- **Domain:** Curated Postgres learning DBs beyond DVD Rental.
- **Source:** https://github.com/neondatabase/postgres-sample-dbs · https://neon.com/postgresql/getting-started/sample-database
- **Source type / license:** Neon teaching collection (per-dataset; several CC-BY-SA).
- **Contents worth mining:**
  - **Pagila** — the "extensive" DVD-rental DB (**33 tables**, BSD) with **range-partitioned `payment` tables by date** and richer views — partitioning showcase.
  - **Lego** (633k rows) — sets/themes/parts/colors/inventories with **self-referencing `themes` hierarchy** (parent_id) + multi-level inventory junctions — deep hierarchy + BOM analog.
  - **Netflix** (8,807) — flat/denormalized catalog (title, type, cast, country) — full-text/array-column demos.
  - **Titanic / World Happiness / Periodic Table** — small single-table analytical sets for aggregation/window functions.
- **Notable patterns:** Pagila range-partitioning; Lego self-referencing tree + nested M:N; wide flat tables for text/window functions.
- **ForgeDB value:** "Modern Postgres-native" slots — Pagila (advanced features), Lego (deep hierarchy), small sets (lightweight analytics).

---

# Part B — Official ORM / Framework Example Schemas

## B1. Prisma — Classic Blog

- **Domain:** Blogging platform (users author posts, categorized, with profiles).
- **Source:** https://www.prisma.io/docs/orm/prisma-schema/data-model/models
- **Source type / license:** Official docs; prisma-examples repo **Apache-2.0**.
- **Core entities &amp; relationships:** `User 1—1 Profile`; `User 1—* Post`; `Post *—* Category` (implicit join table).
- **Notable patterns:** one-to-one, one-to-many, implicit M:N, **enum** (`Role {USER, ADMIN}`), created/updated **timestamps**, `published: Boolean`, nullable fields, cuid()/autoincrement IDs.
- **ForgeDB value:** The canonical "blog" reference — the default content-CMS slot every corpus needs.

## B2. Prisma — Ecommerce (finding: none official)

- **Finding:** The official `prisma/prisma-examples` repo has **no canonical ecommerce schema** — examples are architecture demos reusing the blog model. https://github.com/prisma/prisma-examples (Apache-2.0). Community ecommerce schemas exist but are unofficial. **Author the ecommerce slot from Rails/Supabase storefront patterns instead.**

## B3. Drizzle ORM — Getting Started (users/posts)

- **Domain:** Blog/content — users owning posts.
- **Source:** https://orm.drizzle.team/docs/sql-schema-declaration · https://orm.drizzle.team/docs/relations
- **Source type / license:** Official docs; Drizzle is **Apache-2.0**.
- **Core entities:** `users` (id, firstName, lastName, email, role) 1—* `posts` (authorId FK; fields slug, title, content, publishedAt, createdAt).
- **Notable patterns:** explicit FK `references()`, **secondary index on authorId**, `role` union/enum, `slug` handle, `publishedAt` nullable vs `createdAt` default-now — demos **index + FK + timestamp** together.
- **ForgeDB value:** Lean modern take on the blog slot; minimal "users↔posts with an index" reference.

## B4. TypeORM — Getting Started (User / Photo)

- **Domain:** Photo gallery — users own photos, photos carry metadata.
- **Source:** https://typeorm.io/docs/getting-started/ · https://typeorm.io/docs/relations/relations-faq/
- **Source type / license:** Official docs; TypeORM **MIT**.
- **Core entities:** `Photo *—1 User`; `Photo 1—1 PhotoMetadata` (explicit owning JoinColumn).
- **Notable patterns:** **one-to-one with explicit owning side**, many-to-one/one-to-many inverse pairing, **separate metadata satellite table** (height/width/orientation/compressed/isPublished).
- **ForgeDB value:** The **media/asset + 1:1 metadata** slot — exercises one-to-one and satellite-table modeling, distinct from blogs.

## B5. Django — Official Tutorial (Polls: Question / Choice)

- **Domain:** Polls/voting app.
- **Source:** https://docs.djangoproject.com/en/6.0/intro/tutorial02/
- **Source type / license:** Official docs; Django **BSD-3-Clause**.
- **Core entities:** `Question 1—* Choice` (Choice.question FK, cascade delete); Question(question_text, pub_date), Choice(choice_text, votes int default 0).
- **Notable patterns:** minimal **parent↔child FK with cascade delete**, integer **counter field** (votes), derived date method.
- **ForgeDB value:** The smallest possible relational example — "hello world" survey/voting slot; ideal tiny two-entity smoke test.

## B6. Django — Real-world Blog (Django Girls)

- **Domain:** Blog (widely-taught community tutorial).
- **Source:** https://tutorial.djangogirls.org/en/django_models/
- **Source type / license:** Community tutorial; **CC BY-SA**.
- **Core entities:** `Post *—1 User` (author → built-in `auth.User`); Post(author, title, text, created_date, published_date nullable, publish() method).
- **Notable patterns:** FK to a **built-in User/auth table**, **draft vs published** via nullable date, created/published timestamp pair.
- **ForgeDB value:** Python-ecosystem blog reference; showing "reference external/auth user" + draft-state.

## B7. Ruby on Rails — Getting Started

- **Domain (v8):** Product-catalog / stock-notification storefront. **(v7 and earlier: blog Article/Comment.)**
- **Source:** https://guides.rubyonrails.org/getting_started.html · historic https://guides.rubyonrails.org/v7.0/getting_started.html
- **Source type / license:** Official guide; **CC BY-SA 4.0**.
- **Core entities (v8):** `Product 1—* Subscriber` (back-in-stock notifications); `User 1—* Session` (auth: email_address + password_digest); Product(name, description rich-text, featured_image attachment, inventory_count); Subscriber(product_id, email, unsubscribe token).
- **Core entities (v7 blog):** `Article 1—* Comment` (dependent: :destroy); Article(title, body), Comment(commenter, body).
- **Notable patterns:** parent↔child cascade delete, **rich-text + file-attachment** fields, **auth User/Session pair** (password_digest), **token** field.
- **ForgeDB value:** Two slots — classic Article/Comment **commenting/threaded content**, plus modern **inventory + subscribe-for-notifications** storefront.

## B8. Ecto (Elixir/Phoenix) — Getting Started

- **Domain:** Blog / user-content (User + Post).
- **Source:** https://hexdocs.pm/phoenix/ecto.html · https://hexdocs.pm/phoenix/Mix.Tasks.Phx.Gen.Schema.html
- **Source type / license:** Official Phoenix/Ecto docs; Phoenix **MIT**, Ecto **Apache-2.0**.
- **Core entities:** `User 1—* Post` (post.user_id); User(name, email, bio, number_of_pets), Post(title, body text, user_id).
- **Notable patterns:** changeset/validation mindset (email format, required), `text` vs `string` distinction, quirky **integer domain field** (number_of_pets) for `@min`/`@max` demos.
- **ForgeDB value:** BEAM/Elixir blog slot; validation-first framing maps to ForgeDB `@` directives.

## B9. Supabase — Quickstarts (User Management / Todos)

- **Domain:** Auth-linked user profiles + a to-do starter.
- **Source:** https://supabase.com/docs/guides/getting-started/tutorials/with-react · https://supabase.com/docs/guides/auth/managing-user-data
- **Source type / license:** Official docs / SQL quickstart; Supabase **Apache-2.0**.
- **Core entities:**
  - **User Management:** `profiles.id` is PK **and** FK → `auth.users(id)` cascade — a **1—1 extension of the auth table**. Fields: username (unique), full_name, avatar_url, website; check `char_length(username) >= 3`.
  - **Todos:** `todos`(id, user_id → auth.users, task, is_complete default false) — `auth.users 1—* todos`.
- **Notable patterns:** **shared-PK 1:1 with external auth table**, **unique username + length check** (`&amp;` unique + `@min`), **RLS/ownership** semantics, boolean completion flag, uuid PKs.
- **ForgeDB value:** Fills **auth-profile extension** + **per-user todo/ownership** slots — uuid PK-as-FK, unique handle + check, per-row ownership, underrepresented elsewhere.

---

# Part C — Teaching-Site "Database Design Example" Schemas (Domain-Gap Fillers)

> None openly licensed — inspiration only. Redgate data-model blogs (C6/C7) and Visual
> Paradigm normalization guide (C2) are the most rigorously modeled; GeeksforGeeks are
> solid teaching references, lighter on formal diagrams.

## C1. Hospital / Healthcare Management

- **Domain:** Hospital management (patients, doctors, appointments, prescriptions, departments) — healthcare.
- **Source:** https://www.geeksforgeeks.org/sql/how-to-design-er-diagram-for-a-hospital-management-system/ · https://www.red-gate.com/blog/er-diagram-for-hospital-management-system/
- **Source type:** Teaching article (GeeksforGeeks). No open license.
- **Core entities:** `Employee` supertype → `Doctor`/`Nurse`/`Receptionist` subtypes; `Patient`, `Room`, `TestReport`, `Bill`, `Records`. Patient–Doctor **M:N** (consultations); Patient→Bill/TestReport 1:N; Nurse–Room M:N; prescriptions linked to both appointment + patient.
- **Notable patterns:** supertype/subtype role hierarchy, appointment scheduling (date/time/status enum), prescription tied to visit context vs history, billing 1:N, M:N patient-doctor via appointment junction.
- **ForgeDB value:** Regulated-domain / role-hierarchy slot — subtyping + scheduling absent from Northwind/blog.

## C2. Library Management

- **Domain:** Library system ERD → normalization → schema (books, members, loans, authors, copies).
- **Source:** https://guides.visual-paradigm.com/designing-a-library-system-from-erd-to-normalization-to-database-schema/ · https://www.geeksforgeeks.org/dbms/er-diagram-of-library-management-system/
- **Source type:** Tutorial (Visual Paradigm normalization walkthrough). Vendor educational; reference only.
- **Core entities:** `Book`, `Author` (M:N via book_author), `Publisher` (1:N), `Member`, `Loan`, `Staff`. Distinguish `Book` (title/edition) from physical `Copy` (barcode, status).
- **Notable patterns:** **title-vs-copy inventory split** (logical work vs physical instance), M:N book↔author, loan lifecycle (checkout/due/return, overdue status), explicit normalization narrative.
- **ForgeDB value:** Canonical normalization + inventory-instance teaching case; cleaner author M:N than DVD rental.

## C3. School / University / Student Information System

- **Domain:** University DB (students, courses, enrollments, instructors, grades, sections/departments) — education.
- **Source:** https://creately.com/guides/er-diagrams-for-a-university-management-system/ · https://www.edrawsoft.com/article/er-diagrams-for-university-database.html
- **Source type:** Teaching article + template gallery (Creately). Reference only.
- **Core entities:** `Student`, `Course`, `Instructor`, `Department`, `Enrollment`. Student–Course **M:N with grade carried on the junction** (`Enrollment.grade`); Course→Department 1:N; Course→Instructor; Department head 1:1.
- **Notable patterns:** M:N enrollment with **attributes on the relationship** (grade, term, status) — textbook "association entity"; department 1:1 head; sections/offerings per term.
- **ForgeDB value:** The definitive many-to-many-with-payload teaching schema; complements library's simpler M:N.

## C4. Airline Reservation / Flight Booking

- **Domain:** Flight booking (airports, flights, seats, bookings, passengers, payment) — travel/transport.
- **Source:** https://medium.com/@abhishek.manjunath.1999/design-a-database-for-flight-booking-system-from-scratch-8f0e900ac9df · https://www.geeksforgeeks.org/dbms/how-to-design-er-diagrams-for-booking-and-reservation-systems/
- **Source type:** Tutorial (Medium build-from-scratch). Reference only.
- **Core entities:** `Airport`, `Flight_Details` (source/dest airport FKs, depart/arrive datetime), `Travel_Class` (First/Business/Economy + capacity), `Seat_Details`, `Passenger`, `Reservation` (passenger↔seat), `Payment_Status`. One seat → one reservation.
- **Notable patterns:** seat/inventory **reservation** (unique seat-per-flight, no double-booking), **self-referential flight→airport** (two FKs to same table for origin/destination), fare/travel-class tiers with capacity, payment status enum.
- **ForgeDB value:** Scarce-inventory reservation slot (unique seat lock) — distinct from generic order lines.

## C5. Banking / Financial

- **Domain:** Bank management (customers, accounts, transactions, branches, loans) — finance.
- **Source:** https://www.geeksforgeeks.org/dbms/er-diagram-of-bank-management-system/ · https://www.geeksforgeeks.org/dbms/how-to-design-er-diagrams-for-online-banking-and-financial-services/
- **Source type:** Teaching article (GeeksforGeeks). Reference only.
- **Core entities:** `Customer`, `Account` (type/balance/open-close date), `Transaction` (date/amount/type/status), `Branch`, `Loan`, `Employee`/`LoanOfficer`. Customer–Account **M:N** (joint accounts); Account→Branch 1:N; Account→Transaction 1:N.
- **Notable patterns:** transaction ledger with type enum (debit/credit) — natural place for **double-entry** (paired rows or from/to account FKs), joint-account M:N ownership, account status/close-date temporal fields, branch hierarchy.
- **ForgeDB value:** Financial-ledger slot — monetary integrity, enum-typed transactions, joint ownership absent elsewhere.

## C6. Hotel / Room Booking

- **Domain:** Hotel management (hotels, room types, rooms, guests, bookings, payments) — hospitality.
- **Source:** https://www.red-gate.com/blog/data-model-for-hotel-management-system/ · https://www.geeksforgeeks.org/sql/how-to-design-er-diagrams-for-hotel-and-hospitality-management/
- **Source type:** Teaching article (Redgate data-model blog — well-normalized, clear diagram). Reference only.
- **Core entities:** `Hotel` (stars, checkin/checkout time), `RoomType` (name, price-per-night, capacity), `Room` (number, hotel FK, type FK, status), `Guest`, `Booking` (guest FK, room FK, checkin/checkout dates, total price), `Payment`, `Staff`. RoomType↔Amenities M:N.
- **Notable patterns:** **RoomType template vs physical Room inventory** (pricing/capacity on type, status on room), **temporal availability** (date-range bookings, overlap avoidance), booking→multiple payments, amenities M:N.
- **ForgeDB value:** Date-range/temporal-availability booking slot (distinct from airline's discrete seat lock).

## C7. Food Delivery

- **Domain:** Restaurant delivery (restaurants, menu items, orders, drivers/customers) — on-demand commerce.
- **Source:** https://www.red-gate.com/blog/a-restaurant-delivery-data-model/ · https://medium.com/towards-data-engineering/database-design-for-a-food-delivery-app-like-zomato-swiggy-86c16319b5c5
- **Source type:** Teaching article (Redgate data-model blog, detailed/normalized). Reference only.
- **Core entities:** `city`, `restaurant`, `customer`, `category`, `menu_item`, `offer` + `in_offer`, `placed_order` (restaurant/customer FKs, order/ready/delivery timestamps, final_price), `in_order` line items (order↔item, quantity, item_price), `comment`, `status_catalog` + `order_status` (timestamped status history).
- **Notable patterns:** order **line items** (M:N order↔menu_item with quantity/captured price), **status catalog + timestamped status log** (audit trail, not a single enum column), order-lifecycle timestamps, time-ranged offers, captured-price snapshotting.
- **ForgeDB value:** Order-with-line-items + status-history slot; the status-log is a great temporal/audit showcase.

## C8. E-Learning / LMS

- **Domain:** Learning Management System (courses, lessons, enrollments, quizzes, progress) — edtech.
- **Source:** https://www.geeksforgeeks.org/sql/how-to-design-a-database-for-learning-management-system-lms/ · https://www.geeksforgeeks.org/sql/how-to-design-er-diagrams-for-online-learning-management-systems/
- **Source type:** Teaching article (GeeksforGeeks). Reference only.
- **Core entities:** `User` (student/instructor), `Course`, `Module`, `Lesson` (module FK), `Enrollment` (user↔course M:N + progress), `Quiz` (course FK), `Question`/`Submission`, `Grade`, `DiscussionForum`/`Post`. Course→Module→Lesson **1:N hierarchy**; User–Course M:N via Enrollment.
- **Notable patterns:** nested content hierarchy (Course→Module→Lesson) — good for ForgeDB `[Model]` one-to-many chains; **progress tracking** per enrollment; quizzes with questions/submissions/grades; self-referential discussion threads.
- **ForgeDB value:** Hierarchical-content + progress-state slot; distinct from university SIS (adds ordered content tree + per-user progress).

## C9. Event Ticketing (bonus)

- **Domain:** Online event ticketing &amp; registration (events, venues, seats/ticket types, orders, attendees) — events.
- **Source:** https://www.geeksforgeeks.org/dbms/how-to-design-a-relational-database-for-online-event-ticketing-and-registration/ · https://www.geeksforgeeks.org/dbms/how-to-design-er-diagrams-for-online-ticketing-and-event-management/
- **Source type:** Teaching article (GeeksforGeeks). Reference only.
- **Core entities:** `Event` (name, date, venue, description), `Venue` (geo, capacity), `Organizer`, `TicketType`/`Ticket` (event FK, type, price, availability), `Attendee`, `Order`/`Payment`. Event→Venue N:1; Event→Ticket 1:N; Attendee↔Event M:N via tickets/orders.
- **Notable patterns:** **ticket-type inventory with availability counters** (overselling prevention), event↔venue capacity constraint, order → multiple tickets, optional per-seat assignment.
- **ForgeDB value:** Capacity-limited event-inventory slot — a third distinct "reservation against limited supply" flavor alongside airline seats and hotel rooms.

---

# Part D — MongoDB / NoSQL Sample Datasets (Document-Model Contrast)

> Official MongoDB Atlas sample datasets, https://www.mongodb.com/docs/atlas/sample-data/.
> Provided free for learning in Atlas; docs attach **no explicit open-source license** and
> several derive from third-party public data (Inside Airbnb, NYC OpenData, Crunchbase,
> OpenFlights, Citibike, NOAA). Inspiration only. The consistent contrast vs relational
> samples: **embed one-to-many children as arrays instead of normalizing into join tables**.

## D1. sample_mflix — IMDB-style movie DB

- **Domain:** Movies, theaters, users, comments (mini streaming/catalog app).
- **Source:** https://www.mongodb.com/docs/atlas/sample-data/sample-mflix/
- **Core collections:** `movies` (title, year, genres[], cast[], directors[], embedded `imdb{}`/`awards{}`/`tomatoes{}` rating subdocs), `comments` (**referenced** via `movie_id`, denormalizes commenter name/email), `users`, `sessions` (ref user_id), `theaters` (nested address + GeoJSON point), `embedded_movies` (adds vector `plot_embedding`).
- **Embedded vs referenced:** comments/sessions **referenced**; ratings/genres/cast **embedded** (arrays + subdocs replace 5+ join tables).
- **Notable patterns:** multiple named rating subdocuments on one entity, string arrays replacing M:N junctions, 2dsphere nested geo, mixed embed/reference strategy.
- **ForgeDB value:** Rich embedded subdocs + array attributes on a catalog entity. Media/catalog slot, document-model angle.

## D2. sample_airbnb — rental listings w/ embedded reviews

- **Domain:** Short-term rental marketplace (listing + host + address + reviews).
- **Source:** https://www.mongodb.com/docs/atlas/sample-data/sample-airbnb/ (data from Inside Airbnb).
- **Core collection:** single `listingsAndReviews` — the whole aggregate in one doc. `host{}`, `address{}` (nested GeoJSON), `availability{}`, `review_scores{}`, `images{}`, `amenities[]`, and a **`reviews[]` array of full review subdocs**.
- **Embedded vs referenced:** everything embedded; essentially zero cross-collection joins.
- **Notable patterns:** one-to-many reviews embedded inline (vs relational reviews table + FK), pre-aggregated score rollups, nested geospatial point, parallel pricing scalars.
- **ForgeDB value:** The canonical "embed the whole aggregate" doc — reviews/host/pricing in one. Marketplace/listings slot.

## D3. sample_restaurants — restaurants + neighborhoods

- **Domain:** NYC restaurants w/ health-inspection grades + neighborhood polygons.
- **Source:** https://www.mongodb.com/docs/atlas/sample-data/sample-restaurants/
- **Core collections:** `restaurants` (name, borough, cuisine, `address{coord[]}`, **`grades[]`** inspection array `{date, grade, score}`), `neighborhoods` (GeoJSON **Polygon**).
- **Embedded vs referenced:** grades **embedded** (vs relational inspections child table); restaurant↔neighborhood link is **spatial (`$geoWithin`), not a FK**.
- **Notable patterns:** point vs polygon geometry, embedded time-ordered history array, relationship-as-geometry-containment (no FK).
- **ForgeDB value:** Embedded audit array + geometry-based relationship. Geospatial + inspection-history slot.

## D4. sample_analytics — customers/accounts/transactions (fintech)

- **Domain:** Mock financial services.
- **Source:** https://www.mongodb.com/docs/atlas/sample-data/sample-analytics/
- **Core collections:** `customers` (with `accounts[]` array of IDs + embedded `tier_and_details{}` map), `accounts` (limit, `products[]`), `transactions` (**bucket pattern**: one doc per account-period holding `transactions[]` array + count + date range).
- **Embedded vs referenced:** mixed — customers **reference** accounts (array of IDs = M:N), tier **embedded**, transactions **embedded in time buckets**.
- **Notable patterns:** **bucketing / time-series pattern** (events grouped per doc to bound size) — the standout idea; array-of-FK-IDs M:N; keyed map embedding; decimal amounts as strings.
- **ForgeDB value:** Transaction bucket pattern + array-of-refs M:N — a different answer than a relational per-event transactions table. Fintech/time-series slot.

## D5. sample_training — multi-collection corpus (highlights)

- **Domain:** Grab-bag of real-world public datasets.
- **Source:** https://www.mongodb.com/docs/atlas/sample-data/sample-training/
- **Highlights:**
  - **`routes`** (airline route graph): each doc is an **edge** `{airline{}, src_airport, dst_airport, stops}`, denormalized carrier subdoc — demos `$graphLookup`. Graph/network angle.
  - **`grades`** (student scores): `{class_id, student_id, scores[]}` with `scores[]` of `{type: exam|quiz|homework, score}` — embedded polymorphic score array.
  - **`trips`** (Citibike): start/end station **GeoJSON points**, duration, usertype — paired origin/destination geopoints.
  - (`companies` deep Crunchbase profiles; `posts` with embedded comments[]/tags[]; `inspections`; `zips` legacy loc.)
- **Notable patterns:** edge-list graph docs, embedded child arrays, paired origin/destination geopoints, deep nested profiles.
- **ForgeDB value:** `routes` = graph/edge modeling; `grades`/`posts` = embedded child arrays. Graph-traversal, education, mobility slots.

## D6. sample_supplies — office-supply store sales

- **Domain:** Mock retailer sales/order data.
- **Source:** https://www.mongodb.com/docs/atlas/sample-data/sample-supplies/
- **Core collection:** single `sales` (one doc per order; saleDate, storeLocation, purchaseMethod, couponUsed). **`items[]`** array of line-items `{name, tags[], price, quantity}`; **`customer{}`** embedded snapshot `{gender, age, email, satisfaction}`.
- **Embedded vs referenced:** fully embedded — no separate products/customers collections.
- **Notable patterns:** **embedded order-line-items** (order↔order_line collapsed into one doc), nested `tags[]` inside array elements, denormalized customer snapshot per order.
- **ForgeDB value:** Textbook order + line-items + buyer collapsed into one doc vs relational three-table split. E-commerce/retail orders slot.

## D7. sample_geospatial &amp; sample_weatherdata (brief)

- **sample_geospatial — shipwrecks:** https://www.mongodb.com/docs/atlas/sample-data/sample-geospatial/ — `shipwrecks` with GeoJSON Point (2dsphere) + maritime metadata. Pure point-query. Pure-geospatial slot.
- **sample_weatherdata — data:** https://www.mongodb.com/docs/atlas/sample-data/sample-weather/ — each doc one observation with GeoJSON `position` + **deeply nested measurement subdocs** (airTemperature{value,quality}, wind{direction{},speed{}}, pressure{}, skyCondition{}) + `sections[]`; every measurement pairs value+quality flag. Geospatial + time-series + deeply-nested-observation. IoT/sensor time-series slot.

---

# Part E — Coverage Matrix, Gap Analysis &amp; Top Picks

## Domains already well-covered by classic samples / typical OSS apps

| Domain | Represented by |
|---|---|
| Blog / CMS | Prisma (B1), Drizzle (B3), Django Girls (B6), Ecto (B8), Rails v7 (B7) |
| Film/media rental | DVD Rental (A1), Sakila (A2) |
| Media catalog + billing | Chinook (A4), mflix (D1) |
| Order-management ERP | Northwind (A3), supplies (D6) |
| Enterprise multi-schema | AdventureWorks (A5) |
| HR / temporal history | Employees (A6) |
| Tiny survey/voting | Django Polls (B5) |
| Auth-profile / todos | Supabase (B9) |

## Gap-filling domains (the high-value additions)

| Domain slot | Best source | Distinguishing pattern |
|---|---|---|
| **Healthcare** | Hospital (C1) | supertype/subtype role hierarchy + appointment scheduling |
| **Library / normalization** | Library (C2) | title-vs-copy inventory split, clean author M:N |
| **Education (SIS)** | University (C3) | M:N enrollment with **grade payload on junction** |
| **Airline reservation** | Flight booking (C4) | unique **discrete seat lock**, self-ref flight→airport |
| **Banking / ledger** | Bank (C5) | double-entry / enum-typed transactions, joint-account M:N |
| **Hotel booking** | Hotel (C6) | **date-range temporal availability**, type-vs-room split |
| **Food delivery** | Delivery (C7) | line-items + **timestamped status-log audit trail** |
| **E-learning / LMS** | LMS (C8) | Course→Module→Lesson hierarchy + per-enrollment progress |
| **Event ticketing** | Ticketing (C9) | **capacity-counter** inventory pool |
| **Geospatial + history** | restaurants (D3) | embedded inspection array + geometry-as-relationship |
| **Fintech time-series** | analytics (D4) | bucketing pattern |
| **IoT / sensor** | weatherdata (D7) | deeply nested value+quality observations |

## Distinct modeling patterns available across the corpus (for deliberate showcase)

- **Reservation flavors (pick 2-3 to contrast):** airline unique-seat lock (C4) · hotel date-range availability (C6) · event capacity-counter pool (C9).
- **M:N junctions:** plain (library author, A1 film_actor) · with line attributes (Northwind order_details A3, delivery in_order C7) · with payload (university grade C3) · with effective dates (Employees dept_emp A6).
- **Self-referencing hierarchy:** manager chains (Northwind A3, Chinook A4) · category trees (Lego A7) · flight→airport dual-FK (C4).
- **Supertype/subtype:** AdventureWorks BusinessEntity (A5), Hospital Employee→roles (C1).
- **Temporal / audit:** date-range validity (A6, C6) · timestamped status log (C7) · Pagila partitioning (A7) · inspection history (D3).
- **Inventory split:** logical-vs-physical (library title/copy C2, hotel type/room C6, DVD film/inventory A1).
- **Constraints / directives:** unique handle + length check (Supabase B9) · counters (Django votes B5, ticket availability C9) · validation directives (Ecto B8).
- **Document-model (if ForgeDB grows nested/array support):** embedded child arrays (airbnb reviews D2, supplies items D6) · denormalized snapshots (D6 customer) · geometry-as-relationship (D3) · bucketing (D4).

## Top 6 picks for hand-authored ForgeDB examples

1. **Hospital / Healthcare (C1)** — fills the healthcare gap and is the best showcase for
   **supertype/subtype role hierarchy** + appointment scheduling; a regulated domain that
   signals ForgeDB handles serious modeling, not just blogs.
2. **University / SIS (C3)** — the textbook **M:N-with-payload** (grade on the enrollment
   junction); education slot, and the clearest teaching example of association entities.
3. **Hotel Booking (C6)** — **date-range temporal availability** + type-vs-room inventory
   split; a Redgate-quality well-normalized model and the cleanest "booking" example.
4. **Food Delivery (C7)** — order line-items **plus a timestamped status-log audit trail**
   (not a single enum column); modern on-demand domain and a strong temporal/audit showcase.
5. **Banking / Ledger (C5)** — financial integrity with **enum-typed double-entry
   transactions** and joint-account M:N; a domain notably absent from every classic sample.
6. **Airline Reservation (C4)** — **unique discrete seat lock** (no double-booking) and a
   self-referential flight→airport dual-FK; contrasts hotel's date-range reservation to
   show ForgeDB handling a different constraint style.

*Runners-up:* Library (C2) for a clean normalization teaching piece; LMS (C8) for the
Course→Module→Lesson hierarchy + progress; Event Ticketing (C9) as a third reservation
flavor; Supabase (B9) for the auth-profile + ownership pattern. Keep **one** canonical blog
(Prisma B1) rather than several, since blogs are already saturated.
