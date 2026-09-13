# banking-ledger

A retail banking ledger covering customer accounts, joint ownership, debit/credit
transactions, double-entry-style transfers, and periodic statements.

**Domain:** Banking / Financial Services  
**Provenance:** Synthetic (modeled after the standard banking ledger design pattern)

## Models and key relationships

| Model | Role |
|---|---|
| `Customer` | Account holder; linked to accounts via `[AccountOwner]` |
| `Account` | Bank account with balance (`decimal`, exact), currency (`string`, exactly 3 chars), and lifecycle dates |
| `AccountOwner` | Explicit join model enabling joint accounts (Customer × Account M2M) with a `role` field |
| `Transaction` | Single debit or credit entry on an `Account`, keyed by its posting instant (`id: +timestamp(us)`) |
| `Transfer` | Double-entry move: carries `from_account: *Account` and `to_account: *Account` FKs |
| `Statement` | Periodic summary snapshot for an account |

Key relationships:
- `AccountOwner` bridges `Customer` and `Account` M2M, allowing multiple customers on one account (joint accounts) and one customer to hold many accounts
- `Transfer` references the `Account` model twice (`from_account` / `to_account`) — a dual-FK self-referential pattern
- `Transaction` is the fine-grained append ledger; `Statement` is the coarser periodic rollup

## Grammar features showcased

- `string @length(3, 3)` for ISO 4217 currency codes (`Account.currency`) — a currency code is text, so it is a `string` with an exact length, not a `bytes(3)`
- Explicit M2M join model (`AccountOwner`) with a payload field (`role`)
- Dual FK to the same model (`Transfer.from_account: *Account`, `Transfer.to_account: *Account`)
- `decimal` for every monetary amount (`Account.balance`, `Transaction.amount`, `Transfer.amount`, both `Statement` balances): exact fixed-point on a 16-byte column, a string on the wire so no precision is lost in JSON, and indexable by a scale-normalized key. The rest of the corpus stores money as `i64` cents; a ledger is where fractions of a cent from interest and allocations must sum exactly, so it is the one that chooses the other idiom
- `decimal @min(>0)` on the two amounts: the exclusive bound is what "strictly positive" means for a continuous type, where `@min(1)` would be a floor of exactly one unit
- `id: +timestamp(us)` on `Transaction`: the identity is the posting instant, allocated as `max(now, last + 1)`, so a burst of entries in one clock tick still gets distinct, strictly increasing keys. It must be named `id` and declared `us` to be a key at all. `occurred_at` stays a separate `+timestamp` stamp: business time and posting time differ for a backdated correction
- `&string @length(10, 20)` unique natural key (`account_number`)
- `timestamp?` nullable lifecycle field (`closed_at`)
- Composite `@index(customer, account)` on `AccountOwner` for fast ownership lookup
- Composite `@index(account, occurred_at)` on `Transaction` for statement generation
- Composite `@index(from_account, occurred_at)` on `Transfer` for outgoing transfer history
- Composite `@index(account, period_start)` on `Statement` for statement retrieval
