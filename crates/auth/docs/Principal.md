An authenticated caller. Everything here is opaque data for handlers — it
carries no enforcement logic. `tenant` has already been cross-checked to
equal the process's tenant by the time a `Principal` exists.
