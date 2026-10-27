// Helpers follow their first calls when possible, after their callers.
//
// A calls Z before B, but B also calls Z and must come first.
// Calling Z again changes nothing. Deep comes before Unrelated.
class C
{
    void A() { Z(); B(); Z(); }
    void B() { Z(); }
    void Deep() {}
    void Unrelated() {}
    void Z() { Deep(); }
}
