import fs from "node:fs";
import path from "node:path";

/** The example schema corpus lives at repo-root examples/ (two dirs up). */
export const EXAMPLES_DIR = path.resolve(process.cwd(), "..", "..", "examples");

export interface ExampleMeta {
  slug: string;
  title: string;
  provenance: string;
  origin: "Adapted" | "Synthetic";
  models: number;
  showcases: string;
}

export interface Example extends ExampleMeta {
  source: string;
}

/**
 * The catalog metadata mirrors examples/README.md. The `.forge` source itself is
 * read from disk at build time so it never drifts from the real schema.
 */
export const catalog: ExampleMeta[] = [
  { slug: "hr-directory", title: "HR Directory", origin: "Adapted", provenance: "Oracle HR (UPL)", models: 7, showcases: "Geo hierarchy, employee self-reference, a mutual FK cycle, and temporal job history — the small intro example." },
  { slug: "music-store", title: "Music Store", origin: "Adapted", provenance: "Chinook (MIT)", models: 10, showcases: "Many-to-many playlists, an InvoiceLine join-with-payload, self-referential reports_to, @soft_delete, money as i64 cents." },
  { slug: "wholesale-orders", title: "Wholesale Orders", origin: "Adapted", provenance: "Northwind (MIT port)", models: 8, showcases: "An OrderDetail join with a discount payload, self-referential employees, multi-FK orders, and composite indexes." },
  { slug: "dvd-rental", title: "DVD Rental", origin: "Adapted", provenance: "Sakila / Pagila (BSD)", models: 13, showcases: "Two many-to-many relations on one model, dual FK to the same model, a store↔staff mutual cycle, and a geo chain — the most complex example." },
  { slug: "code-hosting", title: "Code Hosting", origin: "Adapted", provenance: "Gitea (MIT)", models: 11, showcases: "Fork-lineage self-reference, PR head/base dual FK, org/team RBAC joins, and issue↔label many-to-many." },
  { slug: "publishing-membership", title: "Publishing & Membership", origin: "Adapted", provenance: "Ghost (MIT)", models: 7, showcases: "@fulltext, three many-to-many pairs, a subscription-billing join, and ISO currency as char(3)." },
  { slug: "social-graph", title: "Social Graph", origin: "Adapted", provenance: "Mastodon (design-inspired)", models: 7, showcases: "Reply-thread self-reference, follow/block join models (dual FK to Account), and notifications." },
  { slug: "student-information-system", title: "Student Information System", origin: "Synthetic", provenance: "Teaching SIS", models: 7, showcases: "A textbook many-to-many-with-payload (Enrollment grade), section/term/course FKs, and GPA constraints." },
  { slug: "healthcare", title: "Healthcare", origin: "Synthetic", provenance: "Synthetic", models: 6, showcases: "Appointments, role-as-string providers, prescriptions and diagnoses, and a composite scheduling index." },
  { slug: "hotel-reservations", title: "Hotel Reservations", origin: "Synthetic", provenance: "Synthetic", models: 6, showcases: "A RoomType template vs Room inventory split, date-range availability, and i64 money." },
  { slug: "food-delivery", title: "Food Delivery", origin: "Synthetic", provenance: "Synthetic", models: 8, showcases: "A struct GeoPoint (required + optional), an OrderItem join, and a timestamped status-event audit log." },
  { slug: "banking-ledger", title: "Banking Ledger", origin: "Synthetic", provenance: "Synthetic", models: 6, showcases: "Double-entry transactions, a Transfer dual FK to Account, joint-account many-to-many, and char(3) currency." },
  { slug: "airline-reservations", title: "Airline Reservations", origin: "Synthetic", provenance: "Synthetic", models: 7, showcases: "Flight dual FK to Airport, a unique-seat composite index (seat lock), and IATA codes as char(3)." },
  { slug: "blog-cms", title: "Blog CMS", origin: "Synthetic", provenance: "Synthetic", models: 5, showcases: "snake_case component refs (tsx:// / jsx:// / api://), self-referential comments and categories, @fulltext, and @soft_delete." },
  { slug: "project-management", title: "Project Management", origin: "Synthetic", provenance: "Synthetic", models: 8, showcases: "An Org→Team→Project→Issue hierarchy, sub-issue self-reference, label many-to-many, and dual composite indexes." },
  { slug: "saas-multitenant", title: "SaaS Multi-tenant", origin: "Synthetic", provenance: "Synthetic", models: 7, showcases: "Per-tenant *Organization scoping, a Membership RBAC join, API keys, and an audit log." },
  { slug: "ecommerce-store", title: "E-commerce Store", origin: "Synthetic", provenance: "Synthetic", models: 9, showcases: "Product variants, CartItem/OrderItem joins, money as i64 minor units, and SKU/order-number natural keys." },
  { slug: "iot-sensors", title: "IoT Sensors", origin: "Synthetic", provenance: "Synthetic", models: 3, showcases: "A +u64 high-volume PK, a fixed array [f64; 3], a struct Calibration, and append-heavy telemetry." },
];

function readSource(slug: string): string {
  const file = path.join(EXAMPLES_DIR, slug, "schema.forge");
  try {
    return fs.readFileSync(file, "utf8");
  } catch {
    return "";
  }
}

export function getExample(slug: string): Example | null {
  const meta = catalog.find((c) => c.slug === slug);
  if (!meta) return null;
  return { ...meta, source: readSource(slug) };
}

export function getAllExamples(): Example[] {
  return catalog.map((m) => ({ ...m, source: readSource(m.slug) }));
}
