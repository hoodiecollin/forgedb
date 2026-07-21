# ForgeDB CLI

Schema-first database with automatic code generation for TypeScript, React, and REST APIs.

## Installation

### Global Installation (Recommended)

```bash
npm install -g @forgedb/cli
```

### Local Installation

```bash
npm install --save-dev @forgedb/cli
```

## Quick Start

### 1. Initialize a new project

```bash
forgedb init
```

This creates:
- `schema.forge` - Your database schema
- `forgedb.toml` - Configuration file
- `.gitignore` - Git ignore rules

### 2. Define your schema

Edit `schema.forge`:

```forge
User {
  id: +uuid
  email: string @unique
  name: string
  password_hash: string
  created_at: timestamp

  posts: [Post]
}

Post {
  id: +uuid
  title: string
  content: text
  author: User
  published: bool
  created_at: timestamp
}
```

### 3. Generate code

```bash
# TypeScript SDK
forgedb generate typescript --output ./src/generated

# REST API (Rust)
forgedb generate api --output ./api

# OpenAPI specification
forgedb generate openapi --output ./openapi.yaml

# React components (requires React/React-DOM)
forgedb generate components --output ./src/components
```

### 4. Start the server

```bash
forgedb serve --port 3000
```

## Commands

| Command | Description |
|---------|-------------|
| `forgedb init` | Initialize new ForgeDB project |
| `forgedb generate <target>` | Generate code (typescript, api, openapi, components) |
| `forgedb serve` | Start database server |
| `forgedb migrate` | Run schema migrations |
| `forgedb validate [schema]` | Validate schema file |
| `forgedb version` | Show version information |
| `forgedb help` | Show help for commands |

## Generated Code

### TypeScript SDK

Type-safe database client:

```typescript
import { Database } from './generated';

const db = new Database('http://localhost:3000');

// Type-safe queries
const user = await db.users.get('user-id');
const posts = await db.posts.list({ published: true });

// Relations
const userPosts = await db.users.posts(user.id);
```

### React Components

Auto-generated React components from schema:

```tsx
import { UserCard } from './components/user/card';

function App() {
  return <UserCard userId="123" />;
}
```

### REST API

Auto-generated Axum REST API with OpenAPI documentation.

## Requirements

- **Node.js**: >= 18.0.0
- **TypeScript**: >= 5.0.0 (required for code generation)
- **React**: >= 18.0.0 (optional, for component generation)
- **React-DOM**: >= 18.0.0 (optional, for component generation)

### Supported Platforms

- macOS (x64, arm64)
- Linux (x64)
- Windows (x64)

## Configuration

Create `forgedb.toml` in your project:

```toml
[database]
data_dir = "./data"
port = 3000

[codegen]
typescript_output = "./src/generated"
api_output = "./api"

[server]
cors_enabled = true
rate_limit = 100
```

## Schema Syntax

### Field Types

- **Primitives**: `string`, `i32`, `i64`, `f64`, `bool`
- **Special**: `uuid`, `timestamp`, `text`, `json`
- **Arrays**: `[Type]`
- **Relations**: `OtherModel`, `[OtherModel]`

### Constraints

- `@unique` - Unique constraint
- `@indexed` - Create index
- `@required` - Non-nullable (default)
- `@optional` - Optional field (use `?` suffix)

### Primary Keys

- `+` prefix - Auto-generated primary key
- Example: `id: +uuid` generates UUID automatically

### Examples

```forge
# One-to-many relationship
Author {
  id: +uuid
  name: string
  books: [Book]
}

Book {
  id: +uuid
  title: string
  author: Author
}

# Optional fields
Profile {
  id: +uuid
  bio: string?
  avatar_url: string?
}

# Indexes and constraints
Product {
  id: +uuid
  sku: string @unique @indexed
  name: string
  price: f64
}
```

## Troubleshooting

### Binary not found

If you see "ForgeDB binary not found":

```bash
npm rebuild @forgedb/cli
```

### Permission denied (macOS/Linux)

```bash
chmod +x ./node_modules/.bin/forgedb
```

### TypeScript errors

Make sure TypeScript is installed:

```bash
npm install --save-dev typescript
```

## Development

### Build from source

```bash
git clone https://github.com/forgedb/forgedb
cd forgedb
cargo build --release
```

### Local testing

```bash
cd npm-package
npm link
```

## Examples

See the [examples directory](https://github.com/forgedb/forgedb/tree/main/examples) for complete sample projects:

- **Blog** - Simple blog with users and posts
- **E-commerce** - Product catalog with orders
- **Social Network** - Users, posts, and followers
- **Todo App** - Task management with categories

## Documentation

Full documentation available at: https://forgedb.io/docs

- [Schema Language Guide](https://forgedb.io/docs/schema)
- [TypeScript SDK](https://forgedb.io/docs/typescript)
- [REST API](https://forgedb.io/docs/api)
- [React Components](https://forgedb.io/docs/react)

## Contributing

Contributions welcome! See [CONTRIBUTING.md](https://github.com/forgedb/forgedb/blob/main/CONTRIBUTING.md)

## License

MIT License - see [LICENSE](LICENSE) for details

## Support

- [GitHub Issues](https://github.com/forgedb/forgedb/issues)
- [Discord Community](https://discord.gg/forgedb)
- [Documentation](https://forgedb.io/docs)
