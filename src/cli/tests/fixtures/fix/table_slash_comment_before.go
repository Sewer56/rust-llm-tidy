// Package demo carries a misaligned table inside line comments.
package demo

// Add returns the sum of the operands.
func Add(a, b int) int {
	// | Stage | Output |
	// | --- | --- |
	// | parse | items |
	// | transform | transformed items |
	return a + b
}
