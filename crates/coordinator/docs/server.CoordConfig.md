Coordinator tunables resolved at process start (#144/#145, epic #126). Not a
per-request or hot-reload knob — bound once when the coordinator opens. All
three fields are schema-blind (a coordinator interprets no schema).
