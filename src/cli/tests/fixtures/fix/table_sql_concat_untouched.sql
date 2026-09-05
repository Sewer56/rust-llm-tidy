-- Concatenation pipes never form table rows: no delimiter row follows.
SELECT first_name || ' ' || last_name AS display_name
FROM users
WHERE note = prefix || suffix;
