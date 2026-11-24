import Foundation

/// A JSON value, as the core's frames carry it. Reading a field that isn't there gives
/// `.null`, so `frame["message"]["id"].string` never fails, as serde_json's pointers
/// don't in the desktop app.
enum JSON: Sendable, Equatable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSON])
    case object([String: JSON])

    init(parsing data: Data) throws {
        self = JSON.of(try JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]))
    }

    private static func of(_ any: Any) -> JSON {
        switch any {
        case let n as NSNumber:
            // JSONSerialization gives booleans as NSNumber too.
            if CFGetTypeID(n) == CFBooleanGetTypeID() { return .bool(n.boolValue) }
            return .number(n.doubleValue)
        case let s as String: return .string(s)
        case let a as [Any]: return .array(a.map(of))
        case let o as [String: Any]: return .object(o.mapValues(of))
        default: return .null
        }
    }

    subscript(key: String) -> JSON {
        if case .object(let o) = self { return o[key] ?? .null }
        return .null
    }

    var string: String? {
        if case .string(let s) = self { return s }
        return nil
    }

    /// The text of a string field, or "" when there's none.
    var text: String { string ?? "" }

    var int: Int? {
        if case .number(let n) = self { return Int(exactly: n.rounded()) }
        return nil
    }

    var double: Double? {
        if case .number(let n) = self { return n }
        return nil
    }

    var bool: Bool? {
        if case .bool(let b) = self { return b }
        return nil
    }

    var array: [JSON] {
        if case .array(let a) = self { return a }
        return []
    }

    var isNull: Bool { self == .null }

    var any: Any {
        switch self {
        case .null: return NSNull()
        case .bool(let b): return b
        case .number(let n):
            if let i = Int(exactly: n) { return i }
            return n
        case .string(let s): return s
        case .array(let a): return a.map(\.any)
        case .object(let o): return o.mapValues(\.any)
        }
    }

    /// The value as compact JSON text.
    var encoded: Data {
        (try? JSONSerialization.data(withJSONObject: any, options: [.fragmentsAllowed, .withoutEscapingSlashes])) ?? Data("null".utf8)
    }

    /// The value as JSON text, for a person to read (an approval's detail that isn't text).
    var pretty: String {
        let data = (try? JSONSerialization.data(withJSONObject: any, options: [.fragmentsAllowed, .prettyPrinted, .sortedKeys, .withoutEscapingSlashes])) ?? Data()
        return String(decoding: data, as: UTF8.self)
    }
}

extension JSON: ExpressibleByStringLiteral, ExpressibleByBooleanLiteral, ExpressibleByIntegerLiteral,
    ExpressibleByDictionaryLiteral, ExpressibleByArrayLiteral, ExpressibleByNilLiteral
{
    init(stringLiteral value: String) { self = .string(value) }
    init(booleanLiteral value: Bool) { self = .bool(value) }
    init(integerLiteral value: Int) { self = .number(Double(value)) }
    init(dictionaryLiteral elements: (String, JSON)...) {
        self = .object(Dictionary(elements, uniquingKeysWith: { _, b in b }))
    }
    init(arrayLiteral elements: JSON...) { self = .array(elements) }
    init(nilLiteral: ()) { self = .null }
}

extension JSON {
    init(_ s: String) { self = .string(s) }
    init(_ i: Int) { self = .number(Double(i)) }
    init(_ b: Bool) { self = .bool(b) }
    init(_ a: [String]) { self = .array(a.map(JSON.string)) }
}
