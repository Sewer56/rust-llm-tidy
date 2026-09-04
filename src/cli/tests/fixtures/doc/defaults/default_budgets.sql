-- SQL comment paragraph that runs past the 240 character budget on
-- purpose so the default run reports it, while each line stays under
-- the eighty character line budget of the check. The extra filler
-- sentences keep the paragraph count above two hundred forty chars.
-- One more filler line keeps the paragraph comfortably over the limit.
SELECT 'a long string of prose that would overflow if measured, but strings never measure' AS quiet;
