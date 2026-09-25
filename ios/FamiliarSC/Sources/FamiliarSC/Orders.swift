import Foundation

// The captain's word is an order. The incident that settled it (2026-09-19): a command was
// given to Felix and the answer was status, plus something about calling paws. A sentence
// like "bring all the ships to paws truck stop, wait there for my next instruction" is not a
// question about the ship; it is two standing orders for every hull the captain flies. This
// file is the typed order and the deterministic reading of it — no model needed, so the
// templated lane reads an order exactly as the on-device lane does. Nothing here writes: the
// orders are put to the captain and filed on ONE tap, the same act as every proposal.

/// One standing order as the host's `orders.json` takes it, plus the two course verbs, and
/// who it is for.
public struct OrderRequest: Equatable, Sendable, Identifiable {
    public enum Verb: String, Equatable, Sendable, CaseIterable {
        case travel, hold, repair, refuel, payLease
        /// The tanker, on the captain's word ("call paws", "allow the pilot to call paws").
        case callPaws
        /// "As you were": the standing course ends and the pilot's doctrine flies again.
        case resume
        /// The captain changes ship (metal#100): given on the hull they leave, filed at a
        /// berth both hulls share. The money aboard goes with them.
        case board
        /// The verb as the host's `orders.json` spells it.
        public var wire: String { self == .callPaws ? "paws" : rawValue }
    }
    public enum Scope: String, Equatable, Sendable {
        /// The hull in view.
        case thisHull = "this-hull"
        /// Every hull the captain flies (the host's `/captains/{id}/orders`).
        case fleet
    }
    public var verb: Verb
    public var station: String?
    /// `now` | `next-docking`; the course verbs are always `now`.
    public var when: String
    public var amount: Int64?
    /// `board`: the ship the captain steps aboard, as named; the host resolves it.
    public var ship: String?
    public var scope: Scope
    public var id: String { "\(scope.rawValue):\(verb.rawValue):\(station ?? ""):\(when):\(amount ?? 0):\(ship ?? "")" }

    public init(verb: Verb, station: String? = nil, when: String? = nil, amount: Int64? = nil, ship: String? = nil, scope: Scope = .thisHull) {
        self.verb = verb
        self.station = station?.trimmingCharacters(in: .whitespaces).nilIfEmpty
        self.when = when ?? ([.travel, .hold, .resume, .callPaws].contains(verb) ? "now" : "next-docking")
        self.amount = amount
        self.ship = ship?.trimmingCharacters(in: .whitespaces).nilIfEmpty
        // A ship change is one hull's act: the hull the captain leaves.
        self.scope = verb == .board ? .thisHull : scope
    }

    /// The wire body the host takes on `POST …/orders`.
    public var body: [String: JSONValue] {
        var b: [String: JSONValue] = ["verb": .string(verb.wire), "when": .string(when)]
        if let station { b["station"] = .string(station) }
        if let amount { b["amount"] = .number(Double(amount)) }
        if let ship { b["ship"] = .string(ship) }
        return b
    }

    /// The order read back, in words.
    public var sentence: String {
        let who = scope == .fleet ? "every hull" : "this hull"
        switch verb {
        case .travel: return "\(who): travel to \(station ?? "?")"
        case .hold: return "\(who): hold" + (station.map { " at \($0)" } ?? " where it is") + " until your next word"
        case .repair: return "\(who): repair " + (when == "now" ? "now" : "at next docking")
        case .refuel: return "\(who): refuel" + (amount.map { " \($0) units" } ?? " full") + (when == "now" ? " now" : " at next docking")
        case .payLease: return "\(who): pay ℳ\(amount ?? 0) down on the lease"
        case .callPaws: return "\(who): call the tanker (paws) now"
        case .resume: return "\(who): as you were — the standing course ends and the pilot flies its own doctrine again"
        case .board: return "the captain leaves this hull for \(ship ?? "?") at the next berth they share — the money aboard goes too"
        }
    }
}

/// Reads an order out of what the captain said. Deterministic and narrow on purpose: the
/// phrasings a captain actually uses, nothing clever, and NIL when the words are a question
/// — a question still goes to the voice. Station names are passed as said; the host resolves
/// them against the exchange's register ("paws truck stop" → `paws-truckstop`).
public enum OrderParser {
    public static func parse(_ text: String) -> [OrderRequest]? {
        var t = text.lowercased()
            .replacingOccurrences(of: "’", with: "'")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        // "Felix, …" — the name is the address, not the order. "Resume, all ships" is not
        // an address: an order word before the comma stays.
        if let comma = t.firstIndex(of: ","), t[..<comma].split(separator: " ").count == 1,
           !orderWords.contains(String(t[..<comma]).trimmingCharacters(in: .whitespaces)) {
            t = String(t[t.index(after: comma)...]).trimmingCharacters(in: .whitespaces)
        }
        // A question is not an order.
        if t.hasSuffix("?") { return nil }
        let fleet = matches(t, #"\b(all (the |of the |of my |my )?(ships|hulls|vessels)|the (whole |entire )?fleet|every (ship|hull)|everyone|everybody)\b"#)
        let scope: OrderRequest.Scope = fleet ? .fleet : .thisHull
        var out: [OrderRequest] = []
        var station: String?
        // Clauses: sentences, semicolons, "then".
        // Clauses: sentences, semicolons, commas, "then", and "and <wait|hold|stay…>" —
        // "take the fleet to tuna prime and wait" is a travel and a hold.
        // "5,000" is one number, not two clauses.
        var joined = t.replacingOccurrences(of: #"(\d),(\d{3})\b"#, with: "$1$2", options: .regularExpression)
        joined = joined.replacingOccurrences(of: " and then ", with: ". ").replacingOccurrences(of: " then ", with: ". ")
        for w in ["wait", "hold", "stay", "remain", "stand by", "park", "repair", "refuel"] {
            joined = joined.replacingOccurrences(of: " and \(w)", with: ". \(w)")
        }
        let clauses = joined
            .split(whereSeparator: { ".;!,\n".contains($0) })
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
        for c in clauses {
            let now = matches(c, #"\b(now|immediately|right away|at once)\b"#)
            // The override words: the captain takes the hold off.
            if matches(c, #"\b(resume|carry on|as you were|back to (?:work|it|business|normal)|(?:return|go back|get back) to (?:normal|regular|usual|routine)(?: operations?| duty| business)?|normal operations|belay (?:that|the hold|the orders?)|cancel (?:the |that |all )?(?:hold|orders?|course)|lift the hold|release (?:the )?hold|stand down(?: the hold)?|free to (?:fly|trade|work)|go about your business|fly (?:as|how) you (?:see fit|like|will))\b"#) {
                out.append(OrderRequest(verb: .resume, scope: scope))
                continue
            }
            // A ship change: "board KBC-04", "go aboard KBC-04", "change ship to KBC-04",
            // "transfer me to KBC-04". Read before the travel below, which would take
            // "move me to KBC-04" for a course to a station.
            if let name = capture(c, #"\b(?:board|go aboard|step aboard|come aboard|change ships? to|switch ships? to|transfer (?:me|the captain|myself) to|move (?:me|the captain|myself) (?:aboard|to))\s+(?:the\s+)?([a-z0-9][a-z0-9' -]*?)(?=\s+(?:and|then|now|please|at|when)\b|\s*$)"#) {
                let n = name.trimmingCharacters(in: .whitespaces)
                if !n.isEmpty {
                    out.append(OrderRequest(verb: .board, ship: n))
                    continue
                }
            }
            // The tanker: "call paws", "allow / let the pilot (to) call paws", "send for the tanker".
            if matches(c, #"\b(call|send for|request|summon|allow|let|permit|authori[sz]e)\b"#) && matches(c, #"\b(paws|tanker|rescue)\b"#) && !matches(c, #"\b(don't|do not|never|no longer)\b"#) {
                out.append(OrderRequest(verb: .callPaws, scope: scope))
                continue
            }
            // A rendezvous — "rendezvous at tuna prime", "meet at paws truck stop", "gather the
            // fleet at foxy's" — is a travel AND a hold: the ships go there and wait for the
            // captain's next word. Without the hold the first to arrive leaves again
            // (KBC-03, 2026-09-20). A rendezvous is the fleet's unless the captain names one hull.
            if matches(c, #"\b(rendezvous|meet(?: up)?|gather|assemble|regroup|muster|converge|rally)\b"#),
               let st = capture(c, #"\b(?:at|in|on|near|to|by)\s+(?:the\s+)?([a-z0-9][a-z0-9' -]*?)(?=\s+(?:and|then|now|immediately|please|with|for|until)\b|\s*$)"#) {
                let name = st.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: " station", with: "")
                if !name.isEmpty {
                    let rvScope: OrderRequest.Scope = matches(c, #"\b(this (ship|hull|one)|you alone|just you)\b"#) ? .thisHull : .fleet
                    station = name
                    out.append(OrderRequest(verb: .travel, station: name, scope: rvScope))
                    out.append(OrderRequest(verb: .hold, station: name, scope: rvScope))
                    continue
                }
            }
            if let st = capture(c, #"\b(?:bring|take|send|fly|go|move|head|proceed|travel|get|steer|come|return|run|route|make for|make way)\b[^.]*?\b(?:to|for|toward|towards|into)\s+(?:the\s+)?([a-z0-9][a-z0-9' -]*?)(?=\s+(?:and|then|now|immediately|please)\b|\s*$)"#)
                ?? capture(c, #"^(?:all (?:the |my |of the )?(?:ships|hulls)|every (?:ship|hull)|the (?:whole |entire )?fleet|everyone|everybody)\s+to\s+(?:the\s+)?([a-z0-9][a-z0-9' -]*?)(?=\s+(?:and|then|now|please)\b|\s*$)"#) {
                let name = st.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: " station", with: "")
                if !name.isEmpty {
                    station = name
                    out.append(OrderRequest(verb: .travel, station: name, scope: scope))
                    continue
                }
            }
            if matches(c, #"\b(wait|hold|stay|remain|stand by|standby|park|loiter|idle)\b"#) && !matches(c, #"\b(don't|do not|never)\b"#) {
                let at = capture(c, #"\b(?:at|in|by)\s+(?:the\s+)?([a-z0-9][a-z0-9' -]*?)(?=\s+(?:for|until|and|then)\b|\s*$)"#)
                let here = matches(c, #"\b(here|where (?:you|we|it) (?:are|is))\b"#)
                out.append(OrderRequest(verb: .hold, station: here ? nil : (at ?? station), scope: scope))
                continue
            }
            if matches(c, #"\brepair(s|ed|ing)?\b"#) {
                out.append(OrderRequest(verb: .repair, when: now ? "now" : "next-docking", scope: scope))
                continue
            }
            if matches(c, #"\b(refuel|fuel up|top (?:up|off)|fill (?:the |her |up the )?tank)\b"#) {
                let units = capture(c, #"\b(\d+)\s*(?:units?|u)\b"#).flatMap { Int64($0) }
                out.append(OrderRequest(verb: .refuel, when: now ? "now" : "next-docking", amount: units, scope: scope))
                continue
            }
            if matches(c, #"\bpay\b"#) && matches(c, #"\b(lease|balance|debt|mortgage)\b"#) {
                let amount = capture(c, #"(?:ℳ|m)?\s?(\d[\d,]*)"#).flatMap { Int64($0.replacingOccurrences(of: ",", with: "")) }
                if let amount, amount > 0 {
                    out.append(OrderRequest(verb: .payLease, when: now ? "now" : "next-docking", amount: amount, scope: scope))
                    continue
                }
            }
        }
        // Dedupe, keep order.
        var seen: Set<String> = []
        out = out.filter { seen.insert($0.id).inserted }
        return out.isEmpty ? nil : out
    }

    /// What the computer says back when it has read an order — one sentence, the order as
    /// read, and what the tap does. Never status.
    public static func readback(_ orders: [OrderRequest]) -> String {
        let read = orders.map(\.sentence).joined(separator: "; ")
        let hulls = orders.contains { $0.scope == .fleet } ? "the pilots fly it ahead of their own doctrine" : "the pilot flies it ahead of its own doctrine"
        let hold = orders.contains { $0.verb == .hold } ? ", and hold until your next word" : ""
        if orders.allSatisfy({ $0.verb == .resume }) {
            return "Order read — \(read). Tap FILE and the standing course is lifted; the doctrine takes the next fold."
        }
        if orders.allSatisfy({ $0.verb == .board }) {
            return "Order read — \(read). Tap FILE and the pilot files it once both ships are berthed at the same station; until then it waits and says why."
        }
        if orders.allSatisfy({ $0.verb == .callPaws }) {
            return "Order read — \(read). Tap FILE and the pilot calls the tanker under your authority; the fee lands on the ledger."
        }
        return "Order read — \(read). Tap FILE and \(hulls)\(hold)."
    }

    /// Words that open an order and are never a name.
    static let orderWords: Set<String> = ["resume", "hold", "wait", "stay", "stop", "repair", "refuel", "go", "bring", "take", "send", "fly", "move", "head", "proceed", "travel", "return", "call", "pay", "belay", "cancel", "board"]

    static func matches(_ s: String, _ pattern: String) -> Bool {
        s.range(of: pattern, options: .regularExpression) != nil
    }
    static func capture(_ s: String, _ pattern: String) -> String? {
        guard let re = try? NSRegularExpression(pattern: pattern), let m = re.firstMatch(in: s, range: NSRange(s.startIndex..., in: s)), m.numberOfRanges > 1,
              let r = Range(m.range(at: 1), in: s) else { return nil }
        return String(s[r])
    }
}

extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
