# wholesale-orders

A wholesale trading company: customers place orders for products supplied by vendors, fulfilled by employees and shippers.

**Domain:** B2B wholesale / ERP-style order management

**Provenance:** Adapted from Northwind trading company sample database (PostgreSQL port)
(https://github.com/pthom/northwind_psql, MIT License)

---

## Models (8)

| Model | Description |
|---|---|
| `Category` | Product category (Beverages, Seafood, …) |
| `Supplier` | Vendor/supplier company |
| `Shipper` | Shipping carrier |
| `Customer` | Buying company; `customer_code` is the Northwind 5-char natural key |
| `Employee` | Staff member; self-referencing `reports_to: ?Employee` org hierarchy |
| `Product` | Catalog item; `unit_price` as i64 cents; optional supplier and category |
| `Order` | Purchase order; links customer, employee, shipper; `freight` as i64 cents |
| `OrderDetail` | Explicit join model (Order ↔ Product with payload) — quantity, snapshot unit_price, discount fraction |

## Key Relationships

- **Employee self-ref:** `reports_to: ?Employee` with reverse collection `subordinates: [Employee]`
- **Product catalog:** Product references optional Supplier and Category
- **Order ↔ OrderDetail ↔ Product:** `OrderDetail` is an explicit join model carrying `unit_price` (snapshot at order time), `quantity i32`, and `discount f64 [0,1]` — NOT a pure M2M because the link carries data
- **Order header:** links optional Customer, Employee, and Shipper; `order_date: +timestamp` auto-set on create
- **Natural key:** `Customer.customer_code: ^&string @length(5, 5)` captures Northwind's 5-char CHAR customer ID
- **Composite indexes:** `@index(customer, order_date)`, `@index(employee, order_date)` on Order; `@index(order, product)` on OrderDetail

## Grammar Features Showcased

- Explicit join model with payload (`OrderDetail`) vs. pure M2M
- `*Model` required FK, `?Model` optional FK across multiple models
- `[Model]` one-to-many reverse collections (Category.products, Supplier.products, etc.)
- Self-referencing optional FK (`reports_to`/`subordinates` on Employee)
- `^&` combined unique+indexed modifier on natural key (`Customer.customer_code`)
- `+uuid` primary keys, `+timestamp` for auto-dated `order_date`
- `i64` for money (unit_price, freight — stored as cents); `f64` for fractional discount
- `@min`/`@max` on numeric fields (discount bounded [0,1])
- `@length` on string fields, `@url` on home_page
- `@default(0)` on inventory counters, `@default(0)` on freight
- Multiple composite model-level indexes `@index(a, b)` per model
- `bool` field without default (discontinued product flag)

## Grammar Limitation Noted

- Northwind's original `Customers.CustomerID` is a CHAR(5) natural key serving as the primary key. ForgeDB uses `+uuid` as the primary key and expresses the natural key as a separate `^&string` field with `@length(5, 5)`.
- Composite primary keys (OrderDetails uses `(OrderID, ProductID)`) are modeled as `+uuid` PK + composite `@index`.
