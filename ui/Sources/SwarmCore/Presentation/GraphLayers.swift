/// Longest-path layers, with input order inside each layer. Missing edges and cycle-closing edges
/// are ignored so a graph can still show invalid input.
public enum GraphLayers {
    public static func layers<ID: Hashable>(_ nodes: [(id: ID, needs: [ID])]) -> [[ID]] {
        let byID = Dictionary(nodes.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        var layer: [ID: Int] = [:]
        var visiting: Set<ID> = []
        func depth(_ id: ID) -> Int {
            if let known = layer[id] { return known }
            visiting.insert(id)
            let needs = (byID[id]?.needs ?? []).filter { byID[$0] != nil && !visiting.contains($0) }
            let value = needs.map { depth($0) + 1 }.max() ?? 0
            visiting.remove(id)
            layer[id] = value
            return value
        }
        // From the last step back, so the edge dropped in a cycle is the one that points to a later file.
        for step in nodes.reversed() { _ = depth(step.id) }
        var result: [[ID]] = []
        for step in nodes {
            let value = layer[step.id] ?? 0
            while result.count <= value { result.append([]) }
            result[value].append(step.id)
        }
        return result
    }

}
