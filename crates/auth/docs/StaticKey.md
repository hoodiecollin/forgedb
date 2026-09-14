One asymmetric public key in PEM form, tagged with the algorithm it verifies
and an optional `kid`.

Held in [`KeySource::StaticPem`]. The PEM is decoded on every verification
that selects this key.
