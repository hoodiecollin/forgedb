# Component Integration Guide

**Feature**: UI Component Integration (Sprint 17)
**Status**: Completed
**Version**: 1.0

---

## Overview

ForgeDB's Component Integration feature allows you to define React components and API routes directly in your schema using a declarative syntax. This creates a single source of truth for your data models, UI components, and API endpoints.

## Table of Contents

- [Syntax Reference](#syntax-reference)
- [Component Types](#component-types)
- [Relations Directive](#relations-directive)
- [Generated Code](#generated-code)
- [CLI Usage](#cli-usage)
- [Best Practices](#best-practices)
- [Examples](#examples)

---

## Syntax Reference

### Basic Component Field

```forge
ModelName {
  field_name: protocol://path
}
```

- **field_name**: The name of the component field (follows same rules as other fields)
- **protocol**: One of `tsx`, `jsx`, or `api`
- **path**: File path relative to your project root (without file extension)

### Component Field with Relations

```forge
ModelName {
  field_name: protocol://path @relations(relation1, relation2)
}
```

The `@relations` directive specifies which relation fields to include in the component props.

---

## Component Types

### 1. TSX Components (`tsx://`)

TypeScript React components (`.tsx` files).

```forge
User {
  id: +uuid
  name: string
  posts: [Post]

  profileCard: tsx://components/user/ProfileCard @relations(posts)
}
```

**Generated file**: `components/user/ProfileCard/page.tsx`

**Props type**:
```typescript
export type UserProfileCardProps = {
  data: User;
  computed?: UserComputed;
  relations?: {
    posts?: Post[];
  };
};
```

### 2. JSX Components (`jsx://`)

JavaScript React components (`.jsx` files).

```forge
Product {
  id: +uuid
  name: string

  thumbnail: jsx://components/product/Thumbnail
}
```

**Generated file**: `components/product/Thumbnail/page.tsx`

**Props type**:
```typescript
export type ProductThumbnailProps = {
  data: Product;
  computed?: ProductComputed;
};
```

### 3. API Routes (`api://`)

Next.js API route handlers.

```forge
Order {
  id: +uuid
  total: f64

  process: api://routes/order/process
}
```

**Generated file**: `routes/order/process/route.ts`

**Handler signature**:
```typescript
export async function POST(req: NextRequest): Promise<NextResponse> {
  // Implementation
}
```

---

## Relations Directive

The `@relations` directive controls which related data is passed to component props.

### No Relations (Default)

```forge
User {
  profileCard: tsx://components/user/ProfileCard
}
```

Props will NOT include any relations:
```typescript
type UserProfileCardProps = {
  data: User;
  computed?: UserComputed;
};
```

### All Relations (`@relations(*)`)

```forge
User {
  posts: [Post]
  comments: [Comment]

  profileCard: tsx://components/user/ProfileCard @relations(*)
}
```

Props will include ALL relation fields:
```typescript
type UserProfileCardProps = {
  data: User;
  computed?: UserComputed;
  relations: {
    posts: Post[];
    comments: Comment[];
  };
};
```

### Specific Relations

```forge
User {
  posts: [Post]
  comments: [Comment]

  profileCard: tsx://components/user/ProfileCard @relations(posts)
}
```

Props will include ONLY specified relations:
```typescript
type UserProfileCardProps = {
  data: User;
  computed?: UserComputed;
  relations?: {
    posts?: Post[];
  };
};
```

### Multiple Specific Relations

```forge
User {
  posts: [Post]
  comments: [Comment]
  followers: [User]

  profileCard: tsx://components/user/ProfileCard @relations(posts, followers)
}
```

Props will include multiple specified relations:
```typescript
type UserProfileCardProps = {
  data: User;
  computed?: UserComputed;
  relations?: {
    posts?: Post[];
    followers?: User[];
  };
};
```

---

## Generated Code

### Directory Structure

After running code generation:

```
project/
├── schema.forge
├── generated/
│   └── sdk/
│       ├── types.ts              # Base model types
│       ├── component-props.ts    # Component props types
│       └── index.ts              # Barrel exports
├── components/                   # React components
│   └── user/
│       └── ProfileCard/
│           └── page.tsx
└── routes/                       # API route handlers
    └── order/
        └── process/
            └── route.ts
```

### Component Stub (Detailed Template)

```tsx
// components/user/ProfileCard/page.tsx
import { UserProfileCardProps } from '@/generated/sdk';

export default function ProfileCard({
  data,
  computed,
  relations
}: UserProfileCardProps) {
  return (
    <div className="profile-card">
      {/* Basic data fields */}
      <div>
        <h2>{data.name}</h2>
        <p>{data.email}</p>
      </div>

      {/* Computed fields */}
      {computed && (
        <div>
          {/* Render computed fields */}
        </div>
      )}

      {/* Relations */}
      {relations?.posts && (
        <div>
          <h3>Posts</h3>
          {relations.posts.map(post => (
            <div key={post.id}>{post.title}</div>
          ))}
        </div>
      )}
    </div>
  );
}
```

### Route Handler Stub

```typescript
// routes/order/process/route.ts
import { NextRequest, NextResponse } from 'next/server';

export async function POST(req: NextRequest) {
  try {
    const body = await req.json();

    // TODO: Implement your logic here

    return NextResponse.json({
      message: 'Not implemented yet'
    });
  } catch (error) {
    return NextResponse.json(
      { error: 'Internal server error' },
      { status: 500 }
    );
  }
}
```

---

## CLI Usage

### Generate All Code

```bash
forgedb generate --schema schema.forge --output ./output
```

This generates:
- Rust API code
- TypeScript SDK
- Component stubs
- Route handler stubs

### Generate Only Component Stubs

```bash
forgedb generate --schema schema.forge --target stubs --output ./output
```

This generates ONLY:
- Component stubs
- Route handler stubs

### Force Overwrite Existing Files

```bash
forgedb generate --schema schema.forge --force --output ./output
```

The `--force` flag overwrites existing files (use with caution).

### Minimal vs Detailed Stubs

The stub generator supports two template modes:

**Minimal**: Just a TODO comment
**Detailed**: Full field rendering with relation loops

Currently defaults to `Detailed`. Future CLI flag: `--stub-template minimal`

---

## Best Practices

### 1. Organize Components by Model

```forge
User {
  card: tsx://components/user/Card
  avatar: tsx://components/user/Avatar
  profile: tsx://components/user/Profile
}
```

This creates a clear structure:
```
components/
└── user/
    ├── Card/
    ├── Avatar/
    └── Profile/
```

### 2. Use Relations Sparingly

Only include relations that the component actually needs:

```forge
// ❌ Bad: Includes all relations even if not needed
User {
  card: tsx://components/user/Card @relations(*)
}

// ✅ Good: Only includes what's needed
User {
  card: tsx://components/user/Card @relations(posts)
}
```

### 3. Naming Conventions

- **Components**: Use PascalCase (e.g., `ProfileCard`, `UserAvatar`)
- **Routes**: Use lowercase with hyphens (e.g., `process-payment`, `send-email`)
- **Field names**: Use camelCase (e.g., `profileCard`, `processPayment`)

### 4. API Route Organization

Group related endpoints:

```forge
Order {
  create: api://routes/order/create
  cancel: api://routes/order/cancel
  refund: api://routes/order/refund
}
```

### 5. Virtual Fields

Component fields are **virtual** - they don't store data in the database. They're only used for code generation.

---

## Examples

### Example 1: Blog Post Card

```forge
Post {
  id: +uuid
  title: string
  content: string
  createdAt: +timestamp
  author: *User
  comments: [Comment]

  // Component that shows post with author info
  card: tsx://components/post/Card @relations(author)
}
```

### Example 2: User Dashboard

```forge
User {
  id: +uuid
  name: string
  email: string
  posts: [Post]
  comments: [Comment]

  // Dashboard with all user activity
  dashboard: tsx://components/user/Dashboard @relations(*)
}
```

### Example 3: Product with Multiple Views

```forge
Product {
  id: +uuid
  name: string
  price: f64
  reviews: [Review]

  // Different views for different contexts
  thumbnail: tsx://components/product/Thumbnail
  detailView: tsx://components/product/Detail @relations(reviews)
  adminView: tsx://components/product/Admin @relations(*)
}
```

### Example 4: Custom Actions

```forge
User {
  id: +uuid
  email: string
  verified: bool

  // Custom email verification endpoint
  verifyEmail: api://routes/user/verify

  // Password reset endpoint
  resetPassword: api://routes/user/reset-password
}
```

---

## Limitations & Future Work

### Current Limitations (Sprint 17)

- Component stubs are generated but not automatically served
- Bun runtime integration deferred to Sprint 24
- No hot reload on schema changes (Sprint 21)
- Route handlers support POST method only

### Future Enhancements (Sprint 24)

- Bun FFI for direct database access from components
- Reverse proxy integration
- Server-side rendering support
- Component hot reload

---

## Troubleshooting

### Issue: "Expected Slash, found X"

**Cause**: Lexer issue with `://` syntax.

**Solution**: Ensure schema uses correct syntax:
```forge
// ✅ Correct
card: tsx://components/user/Card

// ❌ Wrong
card: tsx: //components/user/Card  # Extra space
```

### Issue: "@relations directive requires parameters"

**Cause**: Empty `@relations()` directive.

**Solution**: Either remove the directive or provide parameters:
```forge
// ✅ Correct
card: tsx://components/user/Card @relations(*)
card: tsx://components/user/Card @relations(posts)

// ❌ Wrong
card: tsx://components/user/Card @relations()
```

### Issue: "Component field should be virtual"

**Cause**: Component fields must not be stored in the database.

**Solution**: Component fields are automatically marked as virtual. No action needed.

---

## API Reference

### ComponentProtocol

```rust
pub enum ComponentProtocol {
    Tsx,  // TypeScript React (.tsx)
    Jsx,  // JavaScript React (.jsx)
    Api,  // API route handler (.ts)
}
```

### RelationInclusion

```rust
pub enum RelationInclusion {
    None,                        // No relations
    All,                         // All relations (@relations(*))
    Specific(Vec<String>),      // Specific relations (@relations(field1, field2))
}
```

### ComponentReference

```rust
pub struct ComponentReference {
    pub protocol: ComponentProtocol,
    pub path: String,
    pub relations: RelationInclusion,
}
```

---

## Related Documentation

- [Sprint 17 Summary](../SPRINT17_SUMMARY.md)
- [Example Project](../examples/component-integration/README.md)
- [Sprint 17 Preparation](../archive/sprint-summaries/SPRINT17_PREPARATION.md)

---

**Last Updated**: 2025-10-14
**Version**: 1.0
