# Component Integration Example

This example demonstrates the UI Component Integration feature (Sprint 17) in ForgeDB, which allows you to define React components and API routes directly in your schema.

## Features Demonstrated

### 1. TSX Component Fields
Define TypeScript React components with the `tsx://` protocol:
```forge
User {
  id: +uuid
  name: string
  profileCard: tsx://components/user/ProfileCard @relations(*)
}
```

### 2. JSX Component Fields
Define JavaScript React components with the `jsx://` protocol:
```forge
User {
  avatarView: jsx://components/user/AvatarView @relations(posts)
}
```

### 3. API Route Fields
Define Next.js API routes with the `api://` protocol:
```forge
User {
  updateProfile: api://routes/user/update
}
```

### 4. Relations Directive
The `@relations` directive controls which related data is included in component props:

- `@relations(*)` - Include ALL relations
- `@relations(posts)` - Include only the `posts` relation
- `@relations(author, comments, tags)` - Include specific relations
- No directive - No relations included

## Getting Started

### 1. Parse the Schema

```bash
cargo run -- parse examples/component-integration/schema.forge
```

### 2. Generate TypeScript SDK

This will generate TypeScript types, including component props types:

```bash
cargo run -- generate --schema examples/component-integration/schema.forge --output ./output
```

Generated files will include:
- `generated/sdk/types.ts` - Base TypeScript types for your models
- `generated/sdk/component-props.ts` - Props types for React components
- `generated/sdk/index.ts` - Barrel export file

### 3. Generate Component Stubs

Generate React component stub files:

```bash
cargo run -- generate --schema examples/component-integration/schema.forge --target stubs --output ./output
```

This generates:
- Component files following Next.js App Router convention (`page.tsx`)
- TypeScript props interfaces
- Placeholder component implementations

### 4. Generate API Route Handlers

Generate Next.js API route handlers:

```bash
cargo run -- generate --schema examples/component-integration/schema.forge --target stubs --output ./output
```

This generates:
- Route handler files with Next.js App Router structure
- POST method handlers by default
- Type-safe request/response handling

## Component Props Types

For each component field, ForgeDB generates a TypeScript props type:

```typescript
// Generated from: profileCard: tsx://components/user/ProfileCard @relations(*)
export type UserProfileCardProps = {
  data: User;
  computed?: UserComputed;
  relations: {
    posts: Post[];
    comments: Comment[];
  };
};
```

## File Structure

After generation, you'll have:

```
output/
├── generated/
│   └── sdk/
│       ├── types.ts              # Base types
│       ├── component-props.ts    # Component props types
│       └── index.ts              # Exports
├── components/
│   ├── user/
│   │   ├── ProfileCard/
│   │   │   └── page.tsx
│   │   └── AvatarView/
│   │       └── page.tsx
│   ├── post/
│   │   ├── PreviewCard/
│   │   │   └── page.tsx
│   │   └── FullView/
│   │       └── page.tsx
│   ├── comment/
│   │   └── Item/
│   │       └── page.tsx
│   └── tag/
│       └── Badge/
│           └── page.tsx
└── routes/
    ├── user/
    │   ├── update/
    │   │   └── route.ts
    │   └── delete/
    │       └── route.ts
    ├── post/
    │   ├── publish/
    │   │   └── route.ts
    │   └── unpublish/
    │       └── route.ts
    └── comment/
        ├── edit/
        │   └── route.ts
        └── delete/
            └── route.ts
```

## Example Component Usage

Once generated, you can use the components in your Next.js application:

```typescript
import { UserProfileCardProps } from '@/generated/sdk';

export default function ProfileCard({ data, computed, relations }: UserProfileCardProps) {
  return (
    <div className="profile-card">
      <h2>{data.name}</h2>
      <p>{data.email}</p>
      <p>{data.bio}</p>

      <h3>Posts ({relations.posts.length})</h3>
      <ul>
        {relations.posts.map(post => (
          <li key={post.id}>{post.title}</li>
        ))}
      </ul>

      <h3>Comments ({relations.comments.length})</h3>
      <ul>
        {relations.comments.map(comment => (
          <li key={comment.id}>{comment.text}</li>
        ))}
      </ul>
    </div>
  );
}
```

## Example API Route Usage

```typescript
import { NextRequest, NextResponse } from 'next/server';

export async function POST(req: NextRequest) {
  try {
    const body = await req.json();

    // Your update profile logic here
    // Access the database, validate input, etc.

    return NextResponse.json({
      success: true,
      message: 'Profile updated successfully'
    });
  } catch (error) {
    return NextResponse.json(
      { error: 'Internal server error' },
      { status: 500 }
    );
  }
}
```

## Next Steps

1. Customize the generated component stubs with your UI logic
2. Implement the API route handlers with your business logic
3. Connect to your ForgeDB database using the generated SDK
4. Build your Next.js application with type-safe components and routes

## Benefits

- **Type Safety**: Automatically generated TypeScript types ensure type safety across your stack
- **Convention**: Follows Next.js App Router conventions for components and routes
- **DRY**: Define components and routes once in your schema
- **Consistency**: All components have consistent prop structures
- **Documentation**: Schema serves as documentation for your components and routes
