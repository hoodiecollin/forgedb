# banking-ledger

A retail banking ledger covering customer accounts, joint ownership, debit/credit
transactions, double-entry-style transfers, and periodic statements.

**Domain:** Banking / Financial Services  
**Provenance:** Synthetic (modeled after the standard banking ledger design pattern)

## Models and key relationships

| Model | Role |
|---|---|
| `Customer` | Account holder; linked to accounts via `[AccountOwner]` |
| `Account` | Bank account with balance (i64 cents), currency (`string`, exactly 3 chars), and lifecycle dates |
| `AccountOwner` | Explicit join model enabling joint accounts (Customer × Account M2M) with a `role` field |
| `Transaction` | Single debit or credit entry on an `Account` |
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
- `i64` for all monetary amounts (minor units / cents); `i64 @min(1)` prevents zero-amount transactions
- `&string @length(10, 20)` unique natural key (`account_number`)
- `timestamp?` nullable lifecycle field (`closed_at`)
- Composite `@index(customer, account)` on `AccountOwner` for fast ownership lookup
- Composite `@index(account, occurred_at)` on `Transaction` for statement generation
- Composite `@index(from_account, occurred_at)` on `Transfer` for outgoing transfer history
- Composite `@index(account, period_start)` on `Statement` for statement retrieval
