class PerfApiReminders
{
    void Collect(int[] items)
    {
        var values = new List<int>();
        var counts = new System.Collections.Generic.Dictionary<string, int>();
        var builder = new System.Text.StringBuilder();
        values.Add(items.Length);
        counts["count"] = items.Length;
        builder.Append(items.Length);
    }
}
