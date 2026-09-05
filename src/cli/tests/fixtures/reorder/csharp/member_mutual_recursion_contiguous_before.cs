// Rule: Mutually recursive methods stay contiguous in source order.
//
// Members before reorder:
// - method Ping (calls Pong)
// - method Pong (calls Ping)
// - method Trace
//
// Members after reorder:
// - method Trace
// - method Ping
// - method Pong
//
// Notes:
// - The Ping/Pong cycle emits as one block in file order.
// - The independent Trace sorts ahead of the cycle block.
//
class Router
{
    void Ping(int n) { if (n > 0) Pong(n - 1); }

    void Pong(int n) { if (n > 0) Ping(n - 1); }

    void Trace() { }
}
