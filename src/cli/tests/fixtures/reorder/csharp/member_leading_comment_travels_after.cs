// Rule: A plain comment above a member travels with that member.
//
// Members before reorder:
// - method Check
// - method Seal (calls Check), preceded by a `//` comment
//
// Members after reorder:
// - method Seal, still preceded by its comment
// - method Check
//
// Notes:
// - The comment sits in the caller's leading gap, so the hoist carries
//   it along.
//
class Vault
{

    // Guard: seals the vault.
    void Seal() { Check(); }
    void Check() { }
}
