Current byte offset of the file cursor. [`Self::read_all`] and
[`Self::read_with_validation`] leave it at the end of the file;
[`Self::read_one`] leaves it just past the record it returned.
