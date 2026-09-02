// Indexed calls read like inline links but are plain calls; the links op
// never runs on JavaScript, so every line stays byte-identical.
const first = items[i](count);
const second = items[i](count);
const third = items[i](count);
