Runs the deprecated tombstone-based [`Compactor::compact_model`] on one model.

Against a generated database it reclaims nothing from updates and brings deleted rows back;
use [`Compactor::compact_model_keeping`] instead.
