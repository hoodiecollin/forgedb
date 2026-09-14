A copy of the results from the most recent pass, scheduled or manual, that returned `Ok`.

Empty before the first such pass; a pass that returned `Err` leaves the previous results
in place. Entries with `success == false` are included.
