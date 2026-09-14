A tombstoned superseding version was appended, logically deleting the row
(#66). Emitters pass the pre-delete version's `row_index` so a subscriber
can still materialize what was deleted (the tombstoned row reads as absent).
