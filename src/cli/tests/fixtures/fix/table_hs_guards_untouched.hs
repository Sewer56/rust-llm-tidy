-- Guard runs carry leading pipes but never form a table: the row after
-- the first is another condition, not a delimiter row, so nothing moves.
clamp lo hi x
  | x < lo = lo
  | x > hi = hi
  | otherwise = x
