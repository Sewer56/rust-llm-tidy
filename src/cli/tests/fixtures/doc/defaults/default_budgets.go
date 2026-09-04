// Package defaults demonstrates the default-run text checks: this
// comment paragraph runs past the 240 character budget on purpose so a
// default run reports it, while each line stays under eighty chars.
// The extra filler sentences keep the paragraph character count safely
// above the two hundred forty character limit of the check.
package defaults

// Code lines and string content never measure, only comment prose does.
var quiet = "a long string of prose that would overflow the paragraph budget if measured, but string content never measures"
