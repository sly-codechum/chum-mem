using System;
using System.Collections.Generic;
using System.Threading.Tasks;

namespace App.Services
{
    // WHY: Interface-first design so we can inject fakes in unit tests
    // without spinning up real HTTP connections.
    public interface IDataService
    {
        Task<List<Record>> FetchAllAsync();
        Task<Record?> FindByIdAsync(string id);
    }

    public record Record(string Id, string Name, DateTime CreatedAt);

    /// <summary>
    /// Default implementation backed by an in-memory store.
    /// NOTE: Not thread-safe — wrap in a lock or use ConcurrentDictionary for prod.
    /// </summary>
    public class InMemoryDataService : IDataService
    {
        private readonly Dictionary<string, Record> _store = new();

        public Task<List<Record>> FetchAllAsync()
        {
            return Task.FromResult(new List<Record>(_store.Values));
        }

        public Task<Record?> FindByIdAsync(string id)
        {
            _store.TryGetValue(id, out var record);
            return Task.FromResult(record);
        }

        public void Seed(IEnumerable<Record> records)
        {
            foreach (var r in records)
                _store[r.Id] = r;
            Console.WriteLine($"Seeded {_store.Count} records");
        }
    }
}
