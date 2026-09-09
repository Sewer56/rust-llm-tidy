class PerfSilent
{
    void Build(int[] items)
    {
        var sized = new List<int>(items.Length);
        var copied = new List<int>(items);
        var populated = new List<int> { 1, 2, 3 };
        var text = "new List<int>()";
        // new List<int>()
        sized.Add(text.Length + populated.Count + copied.Count);
    }
}
