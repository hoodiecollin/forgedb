Never fsync on append. Appended records become durable only through an explicit
[`WalManager::flush`].
