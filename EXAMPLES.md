# SinkDB Examples & Getting Started

## Quick Start

### Installation

```bash
# Install SinkDB CLI
cargo install sinkdb-cli

# Verify installation
sinkdb --version
```

### Create Your First App

```bash
# Initialize a new blog project
sinkdb init my-blog --template blog

# Navigate to project
cd my-blog

# Start development server
sinkdb dev
```

Open your browser to `http://localhost:3000` and you'll see:
- API documentation at `/docs`
- Auto-generated CRUD endpoints
- Type-safe queries

### Project Structure

```
my-blog/
├── schema.lang           # Your schema (edit this!)
├── generated/            # Auto-generated code (don't edit)
├── src/
│   ├── computed/        # Implement computed fields
│   └── views/           # UI components
└── data/                # Database files
```

---

## Example 1: Simple Blog

### Step 1: Define Schema

Create `schema.lang`:

```
User {
  id: +uuid
  email: ^&string @email
  username: ^&char(30) @alphanumeric
  password_hash: #argon2(32) @private
  
  bio: string?
  avatar_url: string?
  
  created_at: +timestamp
  updated_at: ~timestamp
  
  posts: [Post]
  comments: [Comment]
  
  // UI components
  profile: jsx://views/UserProfile.jsx
  card: jsx://components/UserCard.jsx
}

Post {
  id: +uuid
  slug: ^&char(100)
  title: ^string
  content: string @fulltext
  excerpt: string?
  
  author: *User
  category: *Category
  tags: [Tag]
  comments: [Comment]
  
  status: string {
    enum: ["draft", "published", "archived"]
    default: "draft"
  }
  
  view_count: +u64
  
  published_at: timestamp?
  created_at: +timestamp
  updated_at: ~timestamp
  
  // Computed
  read_time: u32 @computed
  comment_count: u32 @computed @materialized
  
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
  parent: Comment?
  
  replies: [Comment]
  
  created_at: +timestamp
  updated_at: ~timestamp
}
```

### Step 2: Generate Code

```bash
sinkdb generate
```

This creates:
- `generated/db.rs` - Rust database implementation
- `generated/types.ts` - TypeScript types
- `generated/api.rs` - REST API server
- `generated/openapi.yaml` - API documentation

### Step 3: Implement Computed Fields

The generator created stubs in `src/computed/Post.ts`:

```typescript
// src/computed/Post.ts
import type { Post } from '../generated/types'

export const PostComputed = {
  // Estimate reading time based on word count
  readTime: (post: Post): number => {
    const words = post.content.split(/\s+/).length
    return Math.ceil(words / 200) // 200 words per minute
  },
  
  // Comment count (materialized, updated on post change)
  commentCount: (post: Post, comments: Comment[]): number => {
    return comments.length
  }
}
```

### Step 4: Create UI Components

```jsx
// src/components/PostPreview.jsx
export default function PostPreview({ data, computed }) {
  return (
    <article className="post-preview">
      <h2>
        <a href={`/posts/${data.slug}`}>{data.title}</a>
      </h2>
      
      {data.excerpt && <p>{data.excerpt}</p>}
      
      <div className="meta">
        <span>By {data.author.name}</span>
        <span>{computed.readTime} min read</span>
        <span>{computed.commentCount} comments</span>
        <time>{new Date(data.published_at).toLocaleDateString()}</time>
      </div>
    </article>
  )
}
```

### Step 5: Start Dev Server

```bash
sinkdb dev
```

### Step 6: Use the API

**Create a post:**
```bash
curl -X POST http://localhost:3000/api/posts \
  -H "Content-Type: application/json" \
  -d '{
    "title": "My First Post",
    "content": "This is the content...",
    "author_id": "550e8400-...",
    "category_id": "660e8400-...",
    "status": "published"
  }'
```

**Query posts:**
```bash
# Get all published posts
curl http://localhost:3000/api/posts?status=published

# Filter by category
curl http://localhost:3000/api/posts?category.slug=technology

# Full-text search
curl http://localhost:3000/api/posts?search=machine%20learning

# Include computed fields
curl http://localhost:3000/api/posts/abc?compute=readTime,commentCount
```

**Get user's posts:**
```bash
curl http://localhost:3000/api/users/123/posts
```

---

## Example 2: E-Commerce Platform

### Schema

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
  email: ^&string @email
  password_hash: #argon2(32) @private
  
  first_name: string
  last_name: string
  full_name: string @computed
  
  shipping_address: Address?
  billing_address: Address?
  
  created_at: +timestamp
  last_login: timestamp?
  
  orders: [Order]
  cart: Cart?
  wishlist: [Product]
}

Product {
  id: +uuid
  sku: &char(20)
  name: ^string
  description: string
  
  price: $USD
  compare_at_price: $USD?
  cost: $USD @admin_only
  
  inventory: i32
  low_stock_threshold: u32 {
    default: 10
  }
  
  dimensions: [f64; 3]  // [width, height, depth] in cm
  weight: f64           // in kg
  
  images: [char(200); 10]
  
  category: *Category
  tags: [Tag]
  
  is_low_stock: bool @computed
  profit_margin: $USD @computed
  
  created_at: +timestamp
  updated_at: ~timestamp
  
  // UI
  card: jsx://components/ProductCard.jsx
  detail: jsx://views/ProductDetail.jsx
  quickview: jsx://components/ProductQuickview.jsx
}

Category {
  id: +uuid
  name: &string
  slug: &char(50)
  parent: Category?
  
  products: [Product]
}

Order {
  id: +uuid
  order_number: &char(20)
  
  user: *User
  
  status: string {
    enum: ["pending", "processing", "shipped", "delivered", "cancelled", "refunded"]
    default: "pending"
  }
  
  items: [OrderItem]
  
  subtotal: $USD
  tax: $USD
  shipping_cost: $USD
  discount: $USD
  total: $USD @computed
  
  shipping_address: Address
  billing_address: Address
  
  tracking_number: char(50)?
  
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

Cart {
  id: +uuid
  user: *User
  
  items: [CartItem]
  
  subtotal: $USD @computed
  
  updated_at: ~timestamp
}

CartItem {
  id: +uuid
  cart: *Cart
  product: *Product
  
  quantity: u32
  
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
  helpful_count: +u32
  
  created_at: +timestamp
  updated_at: ~timestamp
}
```

### Computed Field Implementations

```typescript
// src/computed/Product.ts
export const ProductComputed = {
  isLowStock: (product: Product): boolean => {
    return product.inventory < product.low_stock_threshold
  },
  
  profitMargin: (product: Product): USD => {
    return USD.subtract(product.price, product.cost)
  }
}

// src/computed/Order.ts
export const OrderComputed = {
  total: (order: Order): USD => {
    return USD.add(
      order.subtotal,
      order.tax,
      order.shipping_cost
    ).subtract(order.discount)
  }
}

// src/computed/OrderItem.ts
export const OrderItemComputed = {
  lineTotal: (item: OrderItem): USD => {
    return USD.multiply(item.price_at_purchase, item.quantity)
  }
}
```

### Example API Usage

```bash
# Create product
curl -X POST http://localhost:3000/api/products \
  -H "Content-Type: application/json" \
  -d '{
    "sku": "WIDGET-001",
    "name": "Premium Widget",
    "description": "The best widget money can buy",
    "price": { "value": 2999 },
    "cost": { "value": 1200 },
    "inventory": 150,
    "dimensions": [10.5, 5.2, 3.0],
    "weight": 0.5,
    "category_id": "..."
  }'

# Search products
curl http://localhost:3000/api/products?search=widget&price<3000

# Get low-stock products
curl http://localhost:3000/api/products?compute=isLowStock&filter_computed=isLowStock=true

# Create order
curl -X POST http://localhost:3000/api/orders \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "...",
    "items": [
      {
        "product_id": "...",
        "quantity": 2,
        "price_at_purchase": { "value": 2999 }
      }
    ],
    "shipping_address": {
      "street": "123 Main St",
      "city": "San Francisco",
      "state": "CA",
      "zip": "94102",
      "country": "USA"
    },
    "subtotal": { "value": 5998 },
    "tax": { "value": 540 },
    "shipping_cost": { "value": 500 }
  }'

# Get user's orders
curl http://localhost:3000/api/users/123/orders?sort=-created_at

# Update order status
curl -X PATCH http://localhost:3000/api/orders/abc \
  -H "Content-Type: application/json" \
  -d '{
    "status": "shipped",
    "tracking_number": "1Z999AA10123456784"
  }'
```

---

## Example 3: Task Management

### Schema

```
struct Priority {
  level: u8         // 1-5
  color: char(7)    // Hex color
}

User {
  id: +uuid
  email: ^&string @email
  username: ^&char(30)
  
  created_tasks: [Task]
  assigned_tasks: [Task]
  projects: [Project]
}

Project {
  id: +uuid
  name: ^string
  description: string?
  color: char(7)    // Hex color
  
  owner: *User
  members: [User]
  
  tasks: [Task]
  
  created_at: +timestamp
  updated_at: ~timestamp
  archived_at: timestamp?
  
  // Computed
  completion_percent: f64 @computed
  task_count: u32 @computed @materialized
  
  // UI
  board: jsx://views/ProjectBoard.jsx
  settings: jsx://views/ProjectSettings.jsx
}

Task {
  id: +uuid
  title: ^string
  description: string?
  
  project: *Project
  creator: *User
  assignee: User?
  
  status: string {
    enum: ["todo", "in_progress", "review", "done"]
    default: "todo"
  }
  
  priority: Priority
  
  due_date: date?
  completed_at: timestamp?
  
  tags: [Tag]
  comments: [Comment]
  
  created_at: +timestamp
  updated_at: ~timestamp
  
  // Computed
  is_overdue: bool @computed
  days_until_due: i32 @computed
  
  // UI
  card: jsx://components/TaskCard.jsx
  detail: jsx://views/TaskDetail.jsx
}

Tag {
  id: +uuid
  name: &char(30)
  color: char(7)
  
  tasks: [Task]
}

Comment {
  id: +uuid
  content: string
  
  task: *Task
  author: *User
  
  created_at: +timestamp
  updated_at: ~timestamp
}
```

### Computed Fields

```typescript
// src/computed/Task.ts
export const TaskComputed = {
  isOverdue: (task: Task): boolean => {
    if (!task.due_date || task.status === 'done') return false
    return new Date(task.due_date) < new Date()
  },
  
  daysUntilDue: (task: Task): number => {
    if (!task.due_date) return 0
    const now = new Date()
    const due = new Date(task.due_date)
    const diff = due.getTime() - now.getTime()
    return Math.ceil(diff / (1000 * 60 * 60 * 24))
  }
}

// src/computed/Project.ts
export const ProjectComputed = {
  completionPercent: (project: Project, tasks: Task[]): number => {
    if (tasks.length === 0) return 0
    const completed = tasks.filter(t => t.status === 'done').length
    return (completed / tasks.length) * 100
  },
  
  taskCount: (project: Project, tasks: Task[]): number => {
    return tasks.length
  }
}
```

### Usage

```bash
# Create project
curl -X POST http://localhost:3000/api/projects \
  -d '{
    "name": "Product Launch",
    "color": "#3b82f6",
    "owner_id": "..."
  }'

# Create task
curl -X POST http://localhost:3000/api/tasks \
  -d '{
    "title": "Design landing page",
    "project_id": "...",
    "creator_id": "...",
    "assignee_id": "...",
    "priority": {"level": 3, "color": "#fbbf24"},
    "due_date": "2024-10-20",
    "status": "todo"
  }'

# Get my tasks
curl http://localhost:3000/api/tasks?assignee.id=123

# Get overdue tasks
curl http://localhost:3000/api/tasks?compute=isOverdue&filter_computed=isOverdue=true

# Get project with stats
curl http://localhost:3000/api/projects/abc?include=tasks&compute=completionPercent,taskCount

# Update task status
curl -X PATCH http://localhost:3000/api/tasks/xyz \
  -d '{"status": "done"}'

# Get project board view
curl http://localhost:3000/api/projects/abc?view=board
```

---

## Frontend Integration

### React Example

```tsx
// Using generated TypeScript types
import type { User, Post } from './generated/types'

function BlogPost({ postId }: { postId: string }) {
  const [post, setPost] = useState<Post | null>(null)
  
  useEffect(() => {
    // Type-safe API call
    fetch(`/api/posts/${postId}?compute=readTime,commentCount`)
      .then(r => r.json())
      .then(({ data }) => setPost(data))
  }, [postId])
  
  if (!post) return <div>Loading...</div>
  
  return (
    <article>
      <h1>{post.title}</h1>
      <div className="meta">
        <span>{post._computed.readTime} min read</span>
        <span>{post._computed.commentCount} comments</span>
      </div>
      <div dangerouslySetInnerHTML={{ __html: post.content }} />
    </article>
  )
}
```

### Generated Client SDK (Future)

```typescript
// Auto-generated from schema
import { DB } from './generated/client'

const db = new DB('http://localhost:3000')

// Type-safe queries
const posts = await db.posts
  .where({ status: 'published' })
  .where({ 'author.email': 'alice@example.com' })
  .include(['author', 'comments'])
  .compute(['readTime'])
  .sort('-created_at')
  .limit(10)
  .fetch()

// Type-safe mutations
const newPost = await db.posts.create({
  title: 'New Post',
  content: 'Content here',
  author_id: userId,
  status: 'draft'
})

// Relationships
const author = await newPost.author()
const comments = await newPost.comments()
```

---

## Testing

### Generated Test Helpers

```typescript
// test/helpers.ts - auto-generated
import { TestDB } from '../generated/testing'

export function setupTestDB() {
  return new TestDB({
    inMemory: true,
    fixtures: {
      users: [
        { email: 'test@example.com', username: 'testuser' }
      ],
      posts: [
        { title: 'Test Post', author_id: '$users[0].id' }
      ]
    }
  })
}
```

### Test Example

```typescript
import { setupTestDB } from './helpers'

describe('Blog API', () => {
  let db: TestDB
  
  beforeEach(async () => {
    db = await setupTestDB()
  })
  
  afterEach(async () => {
    await db.teardown()
  })
  
  test('create post', async () => {
    const user = db.fixtures.users[0]
    
    const response = await fetch('/api/posts', {
      method: 'POST',
      body: JSON.stringify({
        title: 'New Post',
        content: 'Content',
        author_id: user.id,
        status: 'published'
      })
    })
    
    expect(response.status).toBe(201)
    const { data } = await response.json()
    expect(data.title).toBe('New Post')
  })
  
  test('computed fields', async () => {
    const post = db.fixtures.posts[0]
    const readTime = PostComputed.readTime(post)
    expect(readTime).toBeGreaterThan(0)
  })
})
```

---

## Deployment

### Production Build

```bash
# Build optimized binary
sinkdb build --release

# Output: dist/sinkdb-api
```

### Docker

```dockerfile
FROM rust:1.75 AS builder
WORKDIR /app
COPY . .
RUN cargo install sinkdb-cli
RUN sinkdb build --release

FROM debian:bookworm-slim
COPY --from=builder /app/dist/sinkdb-api /usr/local/bin/
COPY schema.lang /app/
COPY data/ /app/data/

EXPOSE 3000
CMD ["sinkdb-api", "--config", "/app/sinkdb.toml"]
```

### Environment Variables

```bash
DATABASE_PATH=/app/data/db
API_PORT=3000
API_HOST=0.0.0.0
RUST_LOG=info
```

---

## Best Practices

### Schema Design

1. **Use inline structs** for related fixed-size data
2. **Index discriminators** (fields commonly filtered on)
3. **Mark admin-only fields** with `@private` or `@admin_only`
4. **Use computed fields** for derived data
5. **Add validation** via field config blocks
6. **Document complex logic** with comments

### Performance

1. **Batch operations** when possible
2. **Use field selection** to minimize data transfer
3. **Paginate large results**
4. **Cache frequent queries**
5. **Use materialized computed** for expensive calculations

### Security

1. **Never store plaintext passwords** (use `#argon2`)
2. **Mark sensitive fields** with `@private`
3. **Validate all inputs** via schema constraints
4. **Implement authentication** in middleware
5. **Use HTTPS** in production

### Development Workflow

1. **Edit schema** in `schema.lang`
2. **Run `sinkdb dev`** to see changes immediately
3. **Implement stubs** for new computed fields/components
4. **Test thoroughly** before deployment
5. **Create migrations** for schema changes

---

## Troubleshooting

### Schema Validation Errors

```bash
$ sinkdb validate

❌ Error at line 15:
   Field 'email' has invalid type 'string!'
   Did you mean 'string' (nullable) or just 'string' (required)?
```

**Fix**: Remove the `!` suffix. In SinkDB, fields are required by default.

### Missing Implementations

```bash
$ sinkdb validate --strict

❌ Missing implementations:
   - User.fullName (src/computed/User.ts)
```

**Fix**: Run `sinkdb generate stubs` to create the file, then implement.

### Hot Reload Not Working

```bash
# Check file watcher
sinkdb dev --verbose

# Manual regeneration
sinkdb generate --force
```

### Database Corruption

```bash
# Verify database
sinkdb inspect stats

# Restore from WAL
sinkdb recover

# Rebuild from migrations
sinkdb migrate down --all
sinkdb migrate up
```

---

## Learning Resources

- **Documentation**: Full docs at `/docs` when running dev server
- **Examples**: See `/examples` directory in repo
- **Templates**: Use `sinkdb init --template <n>` for starter projects
- **Community**: (TBD - Discord, GitHub Discussions)

---

## Next Steps

1. **Build something!** Start with a simple app
2. **Explore examples** in this document
3. **Read specifications** for deep dives
4. **Contribute** (once open-sourced)
5. **Share feedback** with the team

---

**Document Version**: 0.1.0
**Last Updated**: 2025-10-11
**Status**: Examples & Tutorial
