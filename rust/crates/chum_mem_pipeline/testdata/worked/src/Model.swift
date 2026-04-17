import Foundation
import Combine

// WHY: Protocol-first design lets us swap CoreData for an in-memory
// store during previews and snapshot tests.
protocol Storable {
    associatedtype ID: Hashable
    var id: ID { get }
    func validate() -> Bool
}

/// A single task in the project tracker.
struct Task: Storable {
    let id: UUID
    var title: String
    var completed: Bool

    /// NOTE: Empty titles are invalid — the UI should prevent this.
    func validate() -> Bool {
        return !title.trimmingCharacters(in: .whitespaces).isEmpty
    }
}

class TaskRepository {
    private var store: [UUID: Task] = [:]
    private let subject = PassthroughSubject<Task, Never>()

    func save(_ task: Task) {
        guard task.validate() else {
            print("Skipping invalid task: \(task.id)")
            return
        }
        store[task.id] = task
        subject.send(task)
    }

    func findAll() -> [Task] {
        return Array(store.values).sorted { $0.title < $1.title }
    }

    var updates: AnyPublisher<Task, Never> { subject.eraseToAnyPublisher() }
}
