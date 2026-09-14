WAL Manager — high-level interface for WAL operations.

Wraps a [`WalWriter`] and [`WalReader`] over a single WAL file. The
manager provides the crash-recovery entry point ([`replay`]) and common
lifecycle operations (`flush`, `rotate`, `truncate`).

[`replay`]: WalManager::replay
