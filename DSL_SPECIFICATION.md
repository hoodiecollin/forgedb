# TypeDB Schema Language Specification

## Version 0.1.0

## Table of Contents
1. [Overview](#overview)
2. [Basic Syntax](#basic-syntax)
3. [Type System](#type-system)
4. [Symbols and Operators](#symbols-and-operators)
5. [Directives](#directives)
6. [Relations](#relations)
7. [Inline Structs](#inline-structs)
8. [Computed Fields](#computed-fields)
9. [UI Integration](#ui-integration)
10. [Complete Examples](#complete-examples)

## Overview

The TypeDB schema language is a declarative DSL for defining data models, relationships, constraints, and UI integration. It prioritizes clarity, type safety, and compile-time optimization.

### Design Principles

- **Concise**: Minimal syntax for common patterns
- **Explicit**: Clear intent, no hidden magic
- **Type-safe**: Strong typing throughout
- **Extensible**: Directives for advanced features

## Basic Syntax

### Model Declaration

```
ModelName {
  field_name: type
  another_field: type options
}
```

### Comments

```
// Single line comment

/*
 * Multi-line comment
 */

/**
 * Documentation comment
 * Used for generated docs and AI instructions
 */
```

## Type System

### Primitive Types

**Unsigned Integers**
- `u8` - 8-bit unsigned (0 to 255)
- `u16` - 16-bit unsigned (0 to 65,535)
- `u32` - 32-bit unsigned (0 to 4,294,967,295)
- `u64` - 64-bit unsigned (0 to 18,446,744,073,709,551,615)

**Signed Integers**
- `i8` - 8-bit signed (-128 to 127)
- `i16` - 16-bit signed (-32,768 to 32,767)
- `i32` - 32-bit signed (-2,147,483,648 to 2,147,483,647)
- `i64` - 64-bit signed (-9,223,372,036,854,775,808 to 9,223,372,036,854,775,807)

**Floating Point**
- `f32` - 32-bit IEEE 754 floating point
- `f64` - 64-bit IEEE 754 floating point

**Boolean**
- `bool` - true or false

**String Types**
- `string` - Variable-length UTF-8 string (stored separately)
- `char(N)` - Fixed-length string, N bytes, zero-padded

**Special Types**
- `uuid` - 128-bit UUID (16 bytes)
- `timestamp` - Unix timestamp with microsecond precision (8 bytes, i64)
- `date` - Date without time (4 bytes, i32 days since epoch)
- `duration` - Time duration (8 bytes, i64 nanoseconds)
- `ipv4` - IPv4 address (4 bytes)
- `ipv6` - IPv6 address (16 bytes)

**Financial Types**
```
$USD  - US Dollar (2 decimal places)
$EUR  - Euro (2 decimal places)
$GBP  - British Pound (2 decimal places)
$JPY  - Japanese Yen (0 decimal places)
$CNY  - Chinese Yuan (2 decimal places)
$BTC  - Bitcoin (8 decimal places)
$ETH  - Ethereum (18 decimal places)
```

All financial types are 128-bit fixed-point decimals (16 bytes).

**Hash Types**
```
#sha256(32)   - SHA-256 hash
#sha512(64)   - SHA-512 hash
#md5(16)      - MD5 hash
#sha1(20)     - SHA-1 hash
#blake3(32)   - BLAKE3 hash
#argon2(32)   - Argon2 hash (for passwords)
```

### Nullability

**Default: Required**
```
age: u32          // NOT NULL, required
```

**Optional: ? Suffix**
```
bio: string?      // NULL allowed, optional
avatar: string?   // Optional field
```

### Arrays (Fixed-Size)

Only valid inside inline structs (see Inline Structs section):

```
values: [f64; 100]           // 100 floats
tags: [char(20); 5]          // 5 fixed-size strings
colors: [RGB; 8]             // 8 RGB structs
```

## Symbols and Operators

### Auto-Increment/Generate: `+`

```
id: +u64              // Auto-increment on insert
user_id: +uuid        // Auto-generate UUID on insert
created_at: +timestamp // Auto-set on create
views: +u32           // Can be manually incremented
```

### Auto-Update: `~`

```
updated_at: ~timestamp  // Auto-update on any write
version: ~u32          // Auto-increment on update
```

### Indexed: `^`

```
email: ^string         // Create index for fast lookups
age: ^u32              // Indexed for queries
```

### Unique Constraint: `&`

```
email: &string         // Must be unique
username: &char(20)    // Unique constraint
```

### Combined Symbols

Symbols can be combined:

```
email: ^&string        // Indexed AND unique
user_id: +uuid         // Auto-generate
score: +^u32          // Auto-increment AND indexed
```

**Order matters for readability:**
1. Auto-behavior (`+`, `~`)
2. Index (`^`)
3. Unique (`&`)
4. Type
5. Nullability (`?`)

```
// Good
field: +^&string?

// Also valid but less readable
field: &^+string?
```

### Required Relation: `*`

```
user: *User           // Required relation (NOT NULL FK)
category: *Category   // Must have a category
```

## Directives

Directives provide additional metadata and behaviors. Format: `@directive` or `@directive(args)`.

### Access Control

```
@public          // Visible in API responses
@private         // Never serialized
@admin_only      // Role-based access
@encrypted       // Encrypted at rest
```

### Validation

```
@email           // Must be valid email format
@url             // Must be valid URL
@phone           // Phone number validation
@alphanumeric    // Only letters and numbers
@lowercase       // Auto-convert to lowercase
@uppercase       // Auto-convert to uppercase
@trim            // Trim whitespace
```

### Indexing

```
@fulltext        // Full-text search index
@spatial         // Spatial index (for coordinates)
@gin             // GIN index (for arrays)
@trigram         // Trigram index (fuzzy search)
```

### Computed Fields

```
@computed        // Virtual field, computed on access
@materialized    // Cached computed field
@cached          // Cache the result
```

### Model-Level Directives

```
@soft_delete     // Adds deleted_at field, soft deletes
@audited         // Creates audit trail
@versioned       // Creates version history
```

### Serialization

```
@json_string     // Serialize as string in JSON
@iso8601         // Format timestamps as ISO 8601
@flatten         // Flatten nested struct in JSON
```

### Relationship Behaviors

```
@cascade_delete  // Delete related records
@eager_load      // Load relation by default
@lazy            // Lazy load relation
```

### Field Configuration Blocks

For complex configuration, use a block:

```
email: string {
  validate: email_format
  max_length: 255
  pattern: "^[a-z0-9]+@[a-z]+\.[a-z]{2,}$"
}

age: u32 {
  min: 0
  max: 150
  default: 18
}

role: string {
  enum: ["admin", "user", "guest"]
  default: "user"
}

posts: [Post] {
  cascade_delete: true
  eager_load: false
}
```

## Relations

### One-to-One

```
User {
  id: +uuid
  profile: Profile?    // Optional one-to-one
}

Profile {
  id: +uuid
  user: *User          // Required, creates user_id FK
}
```

**Generated:**
- `Profile.user_id: uuid` column
- Unique constraint on `user_id`

### One-to-Many

```
User {
  id: +uuid
  posts: [Post]        // One user has many posts
}

Post {
  id: +uuid
  user: *User          // Required, creates user_id FK
}
```

**Generated:**
- `Post.user_id: uuid` column
- Index on `user_id`

### Many-to-Many

```
Post {
  id: +uuid
  tags: [Tag]
}

Tag {
  id: +uuid
  posts: [Post]
}
```

**Generated:**
- Junction table `PostTag`
- Columns: `post_id: uuid`, `tag_id: uuid`
- Composite index on both columns

### Explicit Foreign Keys

For more control:

```
Post {
  id: +uuid
  author_id: uuid      // Explicit FK
  author: *User        // Relation reference
}
```

## Inline Structs

Inline structs are fixed-size compound types that live directly in the columnar storage.

### Declaration

```
struct Address {
  street: char(100)
  city: char(50)
  state: char(2)
  zip: char(10)
  country: char(3)
}

struct Location {
  lat: f64
  lon: f64
  name: char(50)
}

struct RGB {
  r: u8
  g: u8
  b: u8
}
```

### Usage in Models

```
User {
  id: +uuid
  address: Address      // Inline, required
  location: Location?   // Inline, optional
}

Product {
  id: +uuid
  color: RGB
  dimensions: [f64; 3]  // [width, height, depth]
}
```

### Constraints

**Allowed in inline structs:**
- Primitive types: `u8`-`u64`, `i8`-`i64`, `f32`, `f64`, `bool`
- Fixed strings: `char(N)`
- Special types: `uuid`, `timestamp`, `date`, `ipv4`, `ipv6`
- Financial types: `$USD`, etc.
- Hash types: `#sha256(32)`, etc.
- Other inline structs (nesting allowed)
- Fixed-size arrays: `[T; N]`

**NOT allowed in inline structs:**
- Variable-length strings: `string`
- Relations: `[Model]`, `Model`
- Dynamic arrays: `[T]` (without size)

### Nested Structs

```
struct Metadata {
  version: u32
  flags: u64
  location: Location    // Nested struct
  checksum: #sha256(32)
}

struct Location {
  lat: f64
  lon: f64
}

Document {
  id: +uuid
  meta: Metadata        // Contains nested Location
}
```

## Computed Fields

Computed fields are virtual fields calculated on access, not stored in the database.

### Declaration

```
User {
  first_name: string
  last_name: string
  full_name: string @computed    // Not stored
  
  posts: [Post]
  post_count: u32 @computed      // Not stored
}
```

### Implementation Contract

**Rust:**
```rust
trait UserComputed {
  fn full_name(first_name: &str, last_name: &str) -> String;
  fn post_count(posts: &[Post]) -> u32;
}
```

**TypeScript:**
```typescript
type UserComputed = {
  fullName: (first: string, last: string) => string
  postCount: (posts: Post[]) => number
}
```

### Built-in Operations (Future)

```
full_name: string @concat(first_name, " ", last_name)
post_count: u32 @count(posts)
total_value: $USD @sum(orders.amount)
avg_rating: f64 @avg(reviews.rating)
```

### Materialized Computed

For expensive computations that should be cached:

```
User {
  posts: [Post]
  post_count: u32 @computed @materialized
}
```

This stores the value but auto-updates when dependencies change.

## UI Integration

Reference UI components directly in the schema.

### Component References

```
User {
  id: +uuid
  name: string
  email: string
  
  // UI views
  card: jsx://components/UserCard.jsx
  profile: jsx://views/UserProfile.jsx
  edit_form: jsx://admin/UserForm.jsx
}

Post {
  id: +uuid
  title: string
  content: string
  
  detail: jsx://views/PostDetail.jsx
  preview: jsx://components/PostPreview.jsx
}
```

### Generated Component Props

Components receive type-safe props:

```typescript
// Auto-generated
type UserCardProps = {
  data: User
  computed?: UserComputed
  relations?: {
    posts?: Post[]
  }
}
```

### Supported Formats

```
jsx://path/to/component.jsx    // React JSX
html://path/to/template.html   // HTML template
svelte://path/to/comp.svelte   // Svelte component
vue://path/to/comp.vue         // Vue component
```

## Complete Examples

### E-Commerce System

```
struct Address {
  street: char(100)
  city: char(50)
  state: char(2)
  zip: char(10)
  country: char(3)
}

struct Location {
  lat: f64
  lon: f64
}

User {
  id: +uuid
  email: ^&string @email @lowercase
  username: ^&char(30) @alphanumeric
  password_hash: #argon2(32) @private
  
  first_name: string
  last_name: string
  full_name: string @computed
  
  shipping_address: Address?
  billing_address: Address?
  
  created_at: +timestamp @public
  updated_at: ~timestamp @public
  last_login: timestamp? @private
  
  orders: [Order]
  order_count: u32 @computed @materialized
  
  // UI
  profile: jsx://views/UserProfile.jsx
  card: jsx://components/UserCard.jsx
}

Product {
  id: +uuid
  sku: &char(20)
  name: ^string
  description: string
  
  price: $USD
  cost: $USD @admin_only
  margin: $USD @computed
  
  inventory: i32
  location: Location @spatial
  
  images: [char(200); 10]  // Up to 10 image URLs
  
  category: *Category
  tags: [Tag]
  reviews: [Review]
  
  avg_rating: f64 @computed
  review_count: u32 @computed
  
  created_at: +timestamp
  updated_at: ~timestamp
  
  // UI
  card: jsx://components/ProductCard.jsx
  detail: jsx://views/ProductDetail.jsx
  editor: jsx://admin/ProductEditor.jsx
}

Category {
  id: +uuid
  name: &string
  slug: &char(50)
  parent: Category?
  
  products: [Product]
}

Tag {
  id: +uuid
  name: &char(30)
  products: [Product]
}

Order {
  id: +uuid
  order_number: &char(20)
  
  user: *User
  status: string {
    enum: ["pending", "processing", "shipped", "delivered", "cancelled"]
    default: "pending"
  }
  
  subtotal: $USD
  tax: $USD
  shipping: $USD
  total: $USD @computed
  
  items: [OrderItem]
  
  shipping_address: Address
  
  created_at: +timestamp
  updated_at: ~timestamp
  shipped_at: timestamp?
  delivered_at: timestamp?
}

OrderItem {
  id: +uuid
  order: *Order
  product: *Product
  quantity: u32
  price_at_purchase: $USD
  
  line_total: $USD @computed
}

Review {
  id: +uuid
  product: *Product
  user: *User
  
  rating: u8 {
    min: 1
    max: 5
  }
  title: string
  content: string
  
  verified_purchase: bool
  
  created_at: +timestamp
  updated_at: ~timestamp
}
```

### Blogging Platform

```
struct SEO {
  title: char(60)
  description: char(160)
  keywords: [char(30); 10]
}

User {
  @soft_delete
  
  id: +uuid
  email: ^&string @email
  username: ^&char(30)
  password_hash: #argon2(32) @private
  
  bio: string?
  avatar_url: string?
  
  role: string {
    enum: ["admin", "editor", "author", "reader"]
    default: "reader"
  }
  
  posts: [Post]
  comments: [Comment]
  
  created_at: +timestamp
  updated_at: ~timestamp
}

Post {
  @versioned
  
  id: +uuid
  slug: ^&char(100)
  title: ^string
  content: string @fulltext
  excerpt: string?
  
  author: *User
  category: *Category
  tags: [Tag]
  
  seo: SEO
  
  status: string {
    enum: ["draft", "published", "archived"]
    default: "draft"
  }
  
  view_count: +u64
  like_count: +u32
  
  comments: [Comment]
  comment_count: u32 @computed @materialized
  
  published_at: timestamp?
  created_at: +timestamp
  updated_at: ~timestamp
  
  // UI
  detail: jsx://views/PostDetail.jsx
  preview: jsx://components/PostPreview.jsx
  editor: jsx://admin/PostEditor.jsx
}

Category {
  id: +uuid
  name: &string
  slug: &char(50)
  description: string?
  
  posts: [Post]
}

Tag {
  id: +uuid
  name: &char(30)
  slug: &char(30)
  
  posts: [Post]
  usage_count: +u32
}

Comment {
  @soft_delete
  
  id: +uuid
  content: string
  
  post: *Post
  author: *User
  parent: Comment?   // For nested comments
  
  replies: [Comment]
  
  created_at: +timestamp
  updated_at: ~timestamp
}
```

## Migration Considerations

When schema changes, the transpiler generates migrations:

**Adding field:**
```
// Before
User { id: +uuid, name: string }

// After
User { id: +uuid, name: string, email: string }

// Generated migration adds column with NULL or default
```

**Changing type (breaking):**
```
// Before
age: u32

// After
age: i32

// Requires manual data migration
```

**Adding relation:**
```
// Adds FK column, generates junction table if many-to-many
```

## Reserved Keywords

The following are reserved and cannot be used as field or model names:

- `struct`
- `model`
- `enum`
- `true`, `false`
- `null`
- Type names: `string`, `bool`, `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64`, `f32`, `f64`, `uuid`, `timestamp`, `date`, `duration`, `ipv4`, `ipv6`, `char`

## Best Practices

1. **Use meaningful names**: `user_id` not `uid`, `created_at` not `ct`
2. **Be explicit about nullability**: Only use `?` when NULL is valid
3. **Index discriminators**: Fields used in WHERE clauses should be indexed
4. **Use inline structs**: Group related fixed-size data
5. **Computed for derived data**: Don't store what can be calculated
6. **Document with comments**: Especially for computed fields and complex logic

## Future Extensions

- Custom validation functions
- Triggers and hooks
- Advanced indexing strategies
- Partitioning hints
- Sharding directives
- Remote HTTP fields (v2)
- AI implementation annotations (v3)

---

**Specification Version**: 0.1.0
**Last Updated**: 2025-10-11
**Status**: Draft
