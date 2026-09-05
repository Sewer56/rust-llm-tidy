// Rule: Methods with no calls between them keep their source order.
//
// Members before reorder:
// - method Zebras
// - method Alpha
// - method Moon
//
// Members after reorder:
// - unchanged
//
// Notes:
// - The method bucket tie-breaks stably, never alphabetically.
//
class Sorter
{
    void Zebras() { }

    void Alpha() { }

    void Moon() { }
}
