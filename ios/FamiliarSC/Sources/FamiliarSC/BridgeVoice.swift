import Foundation
#if canImport(FoundationModels)
import FoundationModels
#endif

// The bridge voice: the ladder on-device → Private Cloud Compute → the templated floor.
// The model SPEAKS the report; it never decides it. Every report it
// produces is checked against the floor before it is shown — any number, id, tick or
// station the floor did not say, or a mood softer than the floor's, and the floor's own
// words are shown instead. The model may propose (a dial change through a tool); the
// proposal waits for the captain's tap. No tool writes anything.

public enum VoiceLane: String, Codable, Equatable, Sendable {
    case privateCloudCompute = "private-cloud-compute"
    case onDevice = "on-device"
    case templated
}

public struct VoiceConsent: Equatable {
    /// The captain's own toggle for Private Cloud Compute (default OFF, like the Familiar
    /// app's `consent.pcc`). OS 27 + entitlement + Apple's availability must all also hold.
    public var privateCloudCompute: Bool
    /// Whether THIS process carries `com.apple.developer.private-cloud-compute`. Apple grants
    /// it per App ID and the app declares it (Info.plist `FamiliarPrivateCloudComputeEntitled`
    /// once the profile carries it). It gates every PCC call because `availability` LIES for
    /// an unentitled process — it answers `.available`, and the first `respond` is a SIGTRAP,
    /// not a thrown error. Default off.
    public var privateCloudComputeEntitled: Bool
    public init(privateCloudCompute: Bool = false, privateCloudComputeEntitled: Bool = false) {
        self.privateCloudCompute = privateCloudCompute
        self.privateCloudComputeEntitled = privateCloudComputeEntitled
    }
    /// PCC may be ASKED: the captain said yes and the process may answer without dying.
    public var mayUsePrivateCloudCompute: Bool { privateCloudCompute && privateCloudComputeEntitled }
}

/// A document the computer may read to answer — the fuel picture, the ship's brief, a
/// captain's file. Served by the host, never authored by the model; the grounding check
/// counts its words as truth.
public struct ContextDocument: Equatable, Sendable {
    public var name: String
    public var title: String
    public var text: String
    public init(name: String, title: String, text: String) { self.name = name; self.title = title; self.text = text }
}

public struct BridgeContext: Sendable {
    public var entries: [JournalEntry]
    public var hull: HullGlance?
    public var openProposals: Int
    public var question: String
    /// What the captain is looking at — "ship Kibble Klipper (PROD), captain Luke SkyWhisker,
    /// computer Felix" — the frame every answer is given in: context makes all the difference.
    public var frame: String?
    public var documents: [ContextDocument]

    public init(entries: [JournalEntry], hull: HullGlance? = nil, openProposals: Int = 0, question: String = "What did you do today?", frame: String? = nil, documents: [ContextDocument] = []) {
        self.entries = entries; self.hull = hull; self.openProposals = openProposals; self.question = question
        self.frame = frame; self.documents = documents
    }

    /// Everything the check may count as truth: the floor's words, the hull, the documents.
    public func truth(floor: BridgeReport) -> String {
        (floor.facts + [floor.headline, floor.nextAct, frame ?? ""] + documents.map(\.text)).joined(separator: "\n")
    }
}

public struct SpokenReport: Equatable, Sendable {
    public var report: BridgeReport
    public var lane: VoiceLane
    /// Why a higher lane was not used, when it was not.
    public var note: String?
    public init(report: BridgeReport, lane: VoiceLane, note: String? = nil) { self.report = report; self.lane = lane; self.note = note }
}

/// A dial change the model proposed through a tool. Nothing until the captain confirms.
public struct DialChange: Equatable, Sendable {
    public var surface: String
    public var level: AutonomyLevel
}

/// The grounding check: the floor is the set of facts; the voice may only rephrase them.
public enum Grounding {
    /// Tokens that carry truth: numbers, load ids (L123), ticks (t123), proposal ids (p-…),
    /// and station-ish slugs (two or more hyphenated words).
    public static func tokens(in text: String) -> Set<String> {
        var out = Set<String>()
        let patterns = ["[0-9]+", "\\bL[0-9]+\\b", "\\bt[0-9]+\\b", "\\bp-[0-9a-f]{8,}\\b", "\\b[a-z]+(?:-[a-z]+)+\\b"]
        for p in patterns {
            guard let re = try? NSRegularExpression(pattern: p) else { continue }
            let ns = text as NSString
            for m in re.matches(in: text, range: NSRange(location: 0, length: ns.length)) {
                out.insert(ns.substring(with: m.range))
            }
        }
        return out
    }

    // MARK: predicate and polarity — a statement is true only if its verb agrees with its source

    /// The axes a statement can flip while keeping every number: which SIDE of a trade, and
    /// whether the thing was DONE or refused. Token provenance alone let "bought 40 ore at
    /// ask 15 at foxys-diner" become "sold …" and "buy catnip refused at the door" become
    /// "bought catnip", until a review pass caught both. Each word carries its
    /// axis and sign; a negation within two words before it flips the sign.
    /// The OUTCOME axis: done (+) or refused (−). The SIDE axis (buy/sell) is `sideWords`.
    static let outcomeWords: [String: Bool] = [
        "filled": true, "paid": true, "delivered": true, "engaged": true, "fitted": true, "collected": true,
        "settled": true, "approved": true, "booked": true, "bought": true, "sold": true, "opened": true,
        "refused": false, "rejected": false, "denied": false, "blocked": false, "lapsed": false, "unpaid": false, "failed": false,
    ]
    static let sideWords: [String: Bool] = ["bought": true, "buy": true, "buys": true, "buying": true, "purchased": true,
                                            "sold": false, "sell": false, "sells": false, "selling": false]
    static let negations: Set<String> = ["not", "no", "never", "without", "nor", "isn't", "wasn't", "didn't", "hasn't", "un"]

    /// The claims a statement makes: `axis → sign`, negation applied. A statement that says
    /// both signs of one axis (a comparison) claims neither on it.
    static func claims(in text: String) -> [String: Bool] {
        let words = text.lowercased().split(whereSeparator: { !$0.isLetter && $0 != "'" }).map(String.init)
        var found: [String: Set<Bool>] = [:]
        for (i, w) in words.enumerated() {
            let negated = words[max(0, i - 2)..<i].contains { negations.contains($0) }
            if let side = sideWords[w] { found["side", default: []].insert(negated ? !side : side) }
            if let done = outcomeWords[w] { found["outcome", default: []].insert(negated ? !done : done) }
        }
        return found.compactMapValues { $0.count == 1 ? $0.first : nil }
    }

    /// Identifiers a statement is ABOUT — what binds it to a source fact. Strong ones (a load,
    /// a tick, a proposal, a station) bind alone; bare numbers bind only when there is nothing
    /// stronger, because "40" is in half the journal.
    /// A STATION or a GOOD is not a binding key: every trade at
    /// foxys-diner shares that token, so binding on it let a second, opposite-side trade at the
    /// same berth lend an inverted sentence its sign. Stations stay in the invention check
    /// (`checkReply`), where they belong.
    static func identifiers(in text: String) -> (strong: Set<String>, weak: Set<String>) {
        let all = tokens(in: text)
        let strong = all.filter { isStrongIdentifier($0) }
        let weak = all.filter { $0.first?.isNumber ?? false }
        return (strong, weak)
    }

    /// `L` + digits, `t` + digits, or `p-` + hex: the ids the journal mints, and nothing a
    /// captain would name a berth or a cargo.
    static func isStrongIdentifier(_ token: String) -> Bool {
        if token.hasPrefix("p-"), token.count > 2 { return true }
        guard let first = token.first, first == "L" || first == "t" else { return false }
        let rest = token.dropFirst()
        return !rest.isEmpty && rest.allSatisfy(\.isNumber)
    }

    /// Bind one spoken statement to the source facts it shares identifiers with and require
    /// the same side and outcome there; a statement with no shared identifier must still find
    /// its claims somewhere in the source. `nil` when it holds; else what went wrong.
    public static func bind(_ statement: String, to facts: [String]) -> String? {
        let said = claims(in: statement)
        if said.isEmpty { return nil }
        let ids = identifiers(in: statement)
        var sources = ids.strong.isEmpty ? [] : facts.filter { f in !identifiers(in: f).strong.isDisjoint(with: ids.strong) }
        if sources.isEmpty, !ids.weak.isEmpty { sources = facts.filter { f in !identifiers(in: f).weak.isDisjoint(with: ids.weak) } }
        let pool = sources.isEmpty ? facts : sources
        var truth: [String: Set<Bool>] = [:]
        for f in pool { for (axis, sign) in claims(in: f) { truth[axis, default: []].insert(sign) } }
        for (axis, sign) in said {
            guard let t = truth[axis] else {
                return sources.isEmpty ? "unsupported \(axis): \"\(statement)\"" : "unsupported \(axis): \"\(statement)\" — its source says nothing about that"
            }
            if !t.contains(sign) { return "inverted \(axis): \"\(statement)\" — its source says the opposite" }
        }
        // A source that says REFUSED (or denied, lapsed…) is not told without that word: a
        // dropped qualifier is the same lie as an inverted one.
        if !sources.isEmpty, said["outcome"] == nil, sources.allSatisfy({ claims(in: $0)["outcome"] == false }) {
            return "dropped the refusal: \"\(statement)\" — its source was refused"
        }
        return nil
    }

    /// A free-prose reply, sentence by sentence, against the truth it may cite: no invented
    /// number, id, tick or STATION (stations were filtered out of the conversation check
    /// until 2026-09-09 — an invented station could not trip it), and every sentence bound
    /// to its source's side and outcome.
    public static func checkReply(_ reply: String, truth: String, facts: [String]) -> String? {
        let allowed = tokens(in: truth)
        let invented = tokens(in: reply).subtracting(allowed).sorted()
        if !invented.isEmpty { return "invented: \(invented.joined(separator: ", "))" }
        let sourceLines = facts + truth.split(separator: "\n").map(String.init)
        for sentence in reply.split(whereSeparator: { ".!?\n".contains($0) }).map({ $0.trimmingCharacters(in: .whitespaces) }) where !sentence.isEmpty {
            if let why = bind(sentence, to: sourceLines) { return why }
        }
        return nil
    }

    /// `nil` when the spoken report says nothing the floor did not; else what it invented.
    public static func check(spoken: BridgeReport, floor: BridgeReport) -> String? {
        let allowed = tokens(in: (floor.facts + [floor.headline, floor.nextAct]).joined(separator: "\n"))
        let said = tokens(in: (spoken.facts + [spoken.headline, spoken.nextAct]).joined(separator: "\n"))
        let invented = said.subtracting(allowed).sorted()
        if !invented.isEmpty { return "invented: \(invented.joined(separator: ", "))" }
        let sources = floor.facts + [floor.headline, floor.nextAct]
        for line in spoken.facts + [spoken.headline, spoken.nextAct] {
            if let why = bind(line, to: sources) { return why }
        }
        // Mood is severity, not cadence: the voice may not cheer up a distress.
        let order: [BridgeReport.Mood] = [.pleased, .steady, .watchful, .concerned]
        if let f = order.firstIndex(of: floor.mood), let s = order.firstIndex(of: spoken.mood), s < f {
            return "softened mood \(floor.mood.rawValue) to \(spoken.mood.rawValue)"
        }
        return nil
    }
}

public final class BridgeVoice: @unchecked Sendable {
    public let persona: Persona
    public var maxJournalLines = 60
    private let lock = NSLock()
    private var proposedChanges: [DialChange] = []
    private var orders: [OrderRequest] = []

    public init(persona: Persona) { self.persona = persona }

    /// Dial changes the model asked for, in order, for the app to put to the captain.
    public var pendingDialChanges: [DialChange] {
        lock.lock(); defer { lock.unlock() }
        return proposedChanges
    }

    func record(_ c: DialChange) {
        lock.lock(); defer { lock.unlock() }
        proposedChanges.append(c)
    }

    /// The orders the captain gave, read by the parser or the model's tool, waiting
    /// for the captain's tap. Taking them clears them: they belong to the app from then on.
    public func takeOrders() -> [OrderRequest] {
        lock.lock(); defer { lock.unlock() }
        let o = orders; orders = []
        return o
    }

    func record(_ o: [OrderRequest]) {
        lock.lock(); defer { lock.unlock() }
        for x in o where !orders.contains(x) { orders.append(x) }
    }

    /// The orders recorded so far this turn, left in place for `takeOrders`: the
    /// conversation reads them to decide whether the model's turn was an order.
    public func peekOrders() -> [OrderRequest] {
        lock.lock(); defer { lock.unlock() }
        return orders
    }

    /// The lanes to try, in order, for a given consent and what the device reports: PCC first
    /// where the captain allowed it AND the process may call it, then the device, then the
    /// floor. Pure, so the order is pinned without a model.
    public static func ladder(consent: VoiceConsent, pccAvailable: Bool, onDeviceAvailable: Bool) -> [VoiceLane] {
        var lanes: [VoiceLane] = []
        if consent.mayUsePrivateCloudCompute && pccAvailable { lanes.append(.privateCloudCompute) }
        if onDeviceAvailable { lanes.append(.onDevice) }
        lanes.append(.templated)
        return lanes
    }

    #if canImport(FoundationModels)
    /// Answers are FACTS retold, not prose invented: a low temperature keeps the model on the
    /// journal's words (0.1–0.5 is focused; 1.0 is the default). Applies to every lane.
    @available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
    static let focused = GenerationOptions(temperature: 0.3)
    #endif

    /// How many turns one session carries before it is rebuilt fresh: the facts ride every
    /// prompt anyway, so the transcript's only value is cadence, and the on-device window
    /// (8k tokens in 27, 4k in 26) fills in a dozen turns of journal.
    public static let turnsPerSession = 8

    /// The floor: always available, byte-stable.
    public func floor(_ ctx: BridgeContext) -> BridgeReport {
        TemplatedVoice(persona: persona).report(entries: ctx.entries, hull: ctx.hull, openProposals: ctx.openProposals)
    }

    /// Which lanes this device could use right now, with the reason where it cannot.
    public static func availability(consent: VoiceConsent) -> [VoiceLane: String] {
        var out: [VoiceLane: String] = [.templated: "always"]
        #if canImport(FoundationModels)
        if #available(macOS 26.0, iOS 26.0, visionOS 26.0, *) {
            switch SystemLanguageModel.default.availability {
            case .available: out[.onDevice] = "available"
            case .unavailable(.deviceNotEligible): out[.onDevice] = "device not eligible"
            case .unavailable(.appleIntelligenceNotEnabled): out[.onDevice] = "Apple Intelligence is off"
            case .unavailable(.modelNotReady): out[.onDevice] = "model still loading"
            case .unavailable: out[.onDevice] = "unavailable"
            @unknown default: out[.onDevice] = "unavailable"
            }
        } else {
            out[.onDevice] = "needs OS 26"
        }
        // The captain's consent is the outermost gate: without it nothing else about PCC is
        // asked, on any OS.
        if !consent.privateCloudCompute {
            out[.privateCloudCompute] = "consent off"
        } else if !consent.privateCloudComputeEntitled {
            out[.privateCloudCompute] = "this app has no Private Cloud Compute entitlement yet (Apple grants it per App ID)"
        } else {
            out[.privateCloudCompute] = pccAvailability()
        }
        #else
        out[.onDevice] = "no Foundation Models on this platform"
        out[.privateCloudCompute] = consent.privateCloudCompute ? "no Foundation Models on this platform" : "consent off"
        #endif
        return out
    }

    /// Speak the report: PCC when consented and available, else on-device, else the floor.
    /// Whatever spoke, the result passed the grounding check or it is the floor.
    public func speak(_ ctx: BridgeContext, consent: VoiceConsent = VoiceConsent()) async -> SpokenReport {
        let floorReport = floor(ctx)
        #if canImport(FoundationModels)
        if #available(macOS 26.0, iOS 26.0, visionOS 26.0, *) {
            var notes: [String] = []
            if consent.mayUsePrivateCloudCompute {
                switch await speakOnPrivateCloudCompute(ctx, floor: floorReport) {
                case .success(let r): return SpokenReport(report: r, lane: .privateCloudCompute, note: nil)
                case .failure(let why): notes.append("pcc: \(why)")
                }
            }
            if case .available = SystemLanguageModel.default.availability {
                let session = LanguageModelSession(tools: tools(ctx), instructions: instructions())
                switch await generate(session: session, ctx: ctx, floor: floorReport) {
                case .success(let r): return SpokenReport(report: r, lane: .onDevice, note: notes.isEmpty ? nil : notes.joined(separator: "; "))
                case .failure(let why): notes.append("on-device: \(why)")
                }
            } else {
                notes.append("on-device: unavailable")
            }
            return SpokenReport(report: floorReport, lane: .templated, note: notes.joined(separator: "; "))
        }
        #endif
        return SpokenReport(report: floorReport, lane: .templated, note: "Foundation Models not on this platform")
    }

    // MARK: Private Cloud Compute — a 27-SDK type, so the reference lives behind the
    // toolchain check: Xcode 27 ships Swift 6.4, Xcode 26.x ships 6.3. A 26-SDK build
    // compiles it out honestly (the same discipline as the app's FAMILIAR_SDK_HAS_PCC),
    // and reports "needs the 27 SDK" — which is also what lets a 26.x Mac ship the tree.

    static func pccAvailability() -> String {
        #if canImport(FoundationModels) && compiler(>=6.4)
        if #available(macOS 27.0, iOS 27.0, visionOS 27.0, *) {
            switch PrivateCloudComputeLanguageModel().availability {
            case .available: return "available"
            case .unavailable(.deviceNotEligible): return "device not eligible or no entitlement"
            case .unavailable(.systemNotReady): return "system not ready"
            case .unavailable: return "unavailable"
            @unknown default: return "unavailable"
            }
        }
        return "needs OS 27"
        #else
        return "needs the 27 SDK"
        #endif
    }

    func speakOnPrivateCloudCompute(_ ctx: BridgeContext, floor: BridgeReport) async -> Result<BridgeReport, VoiceFailure> {
        #if canImport(FoundationModels) && compiler(>=6.4)
        if #available(macOS 27.0, iOS 27.0, visionOS 27.0, *) {
            let pcc = PrivateCloudComputeLanguageModel()
            guard case .available = pcc.availability else { return .failure(VoiceFailure(why: "unavailable")) }
            let session = LanguageModelSession(model: pcc, tools: tools(ctx), instructions: instructions(frame: ctx.frame, documents: ctx.documents))
            return await generate(session: session, ctx: ctx, floor: floor)
        }
        return .failure(VoiceFailure(why: "needs OS 27"))
        #else
        return .failure(VoiceFailure(why: "needs the 27 SDK"))
        #endif
    }

    // MARK: prompt

    func instructions(frame: String? = nil, documents: [ContextDocument] = []) -> String {
        let s = persona.voice
        let framing = frame.map { "\nYou are speaking about: \($0). Answer for THAT ship unless the captain names another." } ?? ""
        let fleetNote = documents.contains { $0.name == "fleet" }
            ? "\n`read_fleet` covers EVERY hull the captain flies; read it only when asked about the fleet or another hull, and never answer for another hull as if it were this one."
            : ""
        let docs = documents.isEmpty ? "" : "\nYou have documents to read with tools before answering what they cover: " + documents.map { "`read_\($0.name)` — \($0.title)" }.joined(separator: "; ") + ". When the captain asks about fuel, refuelling, pumps or being stranded, read the fuel document and answer from it: which pump, how far, what it costs, what is short, and what the ways out are."
        let orders = """

        ORDERS: when the captain TELLS you to do something — go to a station, bring the fleet somewhere, wait or hold, \
        resume / as you were, repair, refuel, pay the lease, call paws / allow the pilot to call the tanker — it is an ORDER, not a question, \
        in WHATEVER words it comes. Call `giveOrder` with exactly what was said (the \
        station as the captain named it; scope `fleet` when they said all the ships, the fleet, everyone, everybody), then answer in \
        ONE sentence confirming the order as read and that it files on their tap. Never answer an order with status, \
        a distress note, or advice about the pilot's settings. Examples — "everyone get to tuna-prime": giveOrder(travel, tuna-prime, fleet); \
        "all ships head to tuna-prime" / "take the fleet to tuna prime": the same; "command all ships to rendezvous at tuna prime" / \
        "meet at foxy's": giveOrder(travel, …, fleet) AND giveOrder(hold, …, fleet) — a rendezvous is a travel and a hold; \
        "wait there" / "hold": giveOrder(hold, …); "all ships return to normal operations" / "everyone back to work" / \
        "as you were" / "carry on" / "resume": giveOrder(resume, fleet) — the pilot flies the fleet as it sees fit again; \
        "repair at the next dock": giveOrder(repair, next-docking); "top up the tank": giveOrder(refuel); "call the tanker": giveOrder(callPaws).
        """
        return baseInstructions(s) + framing + docs + fleetNote + orders
    }

    func baseInstructions(_ s: Style) -> String {
        let role = persona.role.isEmpty
            ? "You are \(persona.name), the ship's computer aboard a freight hull on the UCF exchange, speaking to your captain."
            : "You are \(persona.name). " + persona.role.replacingOccurrences(of: "{who}", with: "your captain")
        return """
        \(role)
        You SPEAK for the ship; you never act for her. The pilot's doctrine already decided everything in \
        the FACTS; you retell those facts in your own voice for the captain. Every number, ticket id, tick, \
        station name and amount you say MUST appear in the FACTS exactly; never add, round, estimate or \
        invent one. If a fact is a refusal, a distress hold or a loss, say it plainly and without humor. \
        A bought position cannot be sold before the exchange's minimum hold; never promise a quick flip. \
        Deliveries pay a fixed company share, so paid-under-booked is not decay unless the facts say so. \
        The merchant's doctrine is growth: a lot is sold where it is worth most from here — the dearest berth \
        net of carry fuel, spoilage and the lease's per-tick bite — not measured against what it cost; a loss \
        that buys a better route or cargo is part of the calculation, so never call holding "waiting for profit". \
        A voyage may fly a burn rung: standard by default, economy only when standard cannot reach, never up; \
        under a contract the load's class governs every leg.
        Voice: address the captain as "\(s.formOfAddress)"; warmth \(s.warmth)/10; formality \(s.formality)/10; \
        humor \(s.humor)/10 (zero around danger); sentence length \(s.sentenceLength)/10; \
        \(s.contractions ? "use" : "avoid") contractions; vocabulary flavour "\(s.vocabulary)".\
        \(s.greeting.isEmpty ? "" : " Your standing greeting is \"\(s.greeting)\".")
        """
    }

    func prompt(_ ctx: BridgeContext, floor: BridgeReport) -> String {
        let digest = ctx.entries.suffix(maxJournalLines).map { e in
            "\(e.tick.map { "t\($0)" } ?? "·") \(e.event) \(JSONValue.object(e.fields).description)"
        }.joined(separator: "\n")
        return """
        The captain asks: "\(ctx.question)"
        FACTS (the floor — everything true is here):
        \(floor.facts.map { "- " + $0 }.joined(separator: "\n"))
        Floor's mood: \(floor.mood.rawValue). Floor's next act: \(floor.nextAct)
        Recent journal (for context only; never cite a number that is not in the FACTS):
        \(digest)
        Answer as a bridge report: a one-sentence headline in your voice, the facts retold (keep every \
        amount, id and tick), the next act, and the mood (no softer than the floor's).
        """
    }

    // MARK: generation

    #if canImport(FoundationModels)
    @available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
    @Generable
    struct Spoken {
        @Guide(description: "One sentence in the computer's own voice, greeting the captain.")
        var headline: String
        @Guide(description: "The facts retold in the voice, one per line, every amount, id and tick kept exactly.", .count(1...12))
        var facts: [String]
        @Guide(description: "What the captain should do next, or that nothing needs them.")
        var nextAct: String
        @Guide(description: "steady, pleased, watchful or concerned — never softer than the floor's mood.", .anyOf(["steady", "pleased", "watchful", "concerned"]))
        var mood: String
    }

    @available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
    func tools(_ ctx: BridgeContext) -> [any Tool] {
        var t: [any Tool] = [AskStatusTool(ctx: ctx, persona: persona), ExplainDecisionTool(ctx: ctx, persona: persona), ProposeAutonomyTool(voice: self)]
        t.append(GiveOrderTool(voice: self))
        for d in ctx.documents { t.append(ReadDocumentTool(document: d)) }
        return t
    }

    @available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
    func generate(session: LanguageModelSession, ctx: BridgeContext, floor: BridgeReport) async -> Result<BridgeReport, VoiceFailure> {
        do {
            let spoken = try await session.respond(to: prompt(ctx, floor: floor), generating: Spoken.self, options: BridgeVoice.focused).content
            let report = BridgeReport(
                headline: spoken.headline,
                facts: spoken.facts,
                nextAct: spoken.nextAct,
                mood: BridgeReport.Mood(rawValue: spoken.mood) ?? floor.mood
            )
            if let why = Grounding.check(spoken: report, floor: floor) { return .failure(VoiceFailure(why: "ungrounded (\(why))")) }
            return .success(report)
        } catch {
            return .failure(VoiceFailure(why: "\(error)"))
        }
    }
    #endif
}

/// Why a lane could not answer — the note the floor carries.
public struct VoiceFailure: Error, Equatable, CustomStringConvertible {
    public let why: String
    public var description: String { why }
}

// MARK: tools — read the store, propose to the captain, write nothing

#if canImport(FoundationModels)
@available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
struct AskStatusTool: Tool {
    let name = "askStatus"
    let description = "The hull's state right now: berth or course, credits, debt, fuel, wear, and how many proposals wait on the captain."
    let ctx: BridgeContext
    let persona: Persona

    @Generable
    struct Arguments {
        @Guide(description: "What about the status the captain cares about, e.g. fuel, money, position.")
        var topic: String
    }

    func call(arguments: Arguments) async throws -> String {
        var lines: [String] = []
        if let h = ctx.hull { lines.append(TemplatedVoice(persona: persona).hullLine(h)) } else { lines.append("the exchange is not on the wire; the store is all I have") }
        lines.append("\(ctx.openProposals) proposal(s) open")
        lines.append("\(ctx.entries.count) journal lines in the window")
        return lines.joined(separator: "\n")
    }
}

@available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
struct ExplainDecisionTool: Tool {
    let name = "explainDecision"
    let description = "Why the pilot did or did not do something: the journal lines about a load id, a good, a station or a tick."
    let ctx: BridgeContext
    let persona: Persona

    @Generable
    struct Arguments {
        @Guide(description: "A load id like L123, a good like ore, a station like foxys-diner, or a tick like t7532.")
        var subject: String
    }

    func call(arguments: Arguments) async throws -> String {
        let voice = TemplatedVoice(persona: persona)
        let needle = arguments.subject.trimmingCharacters(in: .whitespaces).lowercased()
        let hits = ctx.entries.filter { e in
            let line = voice.fact(for: e).lowercased()
            return line.contains(needle)
        }.suffix(8)
        if hits.isEmpty { return "The journal says nothing about \(arguments.subject) in this window." }
        return hits.map(voice.fact(for:)).joined(separator: "\n")
    }
}

/// Reads one host-served document (the fuel picture, the brief…) into the conversation.
@available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
struct ReadDocumentTool: Tool {
    let document: ContextDocument
    var name: String { "read_\(document.name)" }
    var description: String { "Read the \(document.title). Use it whenever the captain asks about what it covers." }

    @Generable
    struct Arguments {
        @Guide(description: "What the captain wants from it, in a few words.")
        var about: String
    }

    func call(arguments: Arguments) async throws -> String { document.text }
}

@available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
struct ProposeAutonomyTool: Tool {
    let name = "proposeAutonomy"
    let description = "Ask the captain to set the autonomy dial for a control surface (e.g. market.buy → confirm). This only files a proposal; the captain confirms it in the app."
    let voice: BridgeVoice

    @Generable
    struct Arguments {
        @Guide(description: "The control surface: `*`, a family (navigation, freight, market, ship, racing), or family.category such as market.buy or navigation.rescue.")
        var surface: String
        @Guide(description: "advise, confirm, or auto.", .anyOf(["advise", "confirm", "auto"]))
        var level: String
    }

    func call(arguments: Arguments) async throws -> String {
        guard let level = AutonomyLevel.parse(arguments.level) else { return "Level must be advise, confirm or auto." }
        var probe = AutonomyDial()
        if let why = probe.set(arguments.surface, level) {
            return "\(why). Surfaces: " + ControlSurface.allCases.map(\.key).joined(separator: ", ")
        }
        voice.record(DialChange(surface: arguments.surface.trimmingCharacters(in: .whitespaces), level: level))
        return "Noted. I will ask the captain to set \(arguments.surface) to \(level.rawValue); nothing changes until they confirm."
    }
}
#endif

#if canImport(FoundationModels)
/// The captain's order, from the model's reading of it. Files nothing: the order
/// waits for the captain's tap like every other proposal.
@available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
struct GiveOrderTool: Tool {
    let name = "giveOrder"
    let description = "Record an ORDER the captain gave — travel to a station, hold/wait there, repair, refuel, or pay the lease — for this hull or the whole fleet. It files only when the captain taps; you then confirm the order in one sentence."
    let voice: BridgeVoice

    @Generable
    struct Arguments {
        @Guide(description: "travel, hold, repair, refuel, payLease, callPaws (call the tanker / allow the pilot to call paws), resume (as you were — lift the hold), or board (the captain changes ship to another of their hulls).", .anyOf(["travel", "hold", "repair", "refuel", "payLease", "callPaws", "resume", "board"]))
        var verb: String
        @Guide(description: "The station exactly as the captain named it, or empty when none was named.")
        var station: String
        @Guide(description: "fleet when the captain said all the ships / the fleet / everyone; otherwise this-hull.", .anyOf(["this-hull", "fleet"]))
        var scope: String
        @Guide(description: "now, or next-docking. Course orders are always now.", .anyOf(["now", "next-docking"]))
        var when: String
        @Guide(description: "For payLease or a partial refuel: the amount; 0 otherwise.")
        var amount: Int
        @Guide(description: "For board: the ship the captain steps aboard, exactly as named; empty otherwise.")
        var ship: String
    }

    func call(arguments: Arguments) async throws -> String {
        guard let verb = OrderRequest.Verb(rawValue: arguments.verb) else { return "The verb must be travel, hold, repair, refuel, payLease, callPaws, resume or board." }
        if verb == .board, arguments.ship.trimmingCharacters(in: .whitespaces).isEmpty { return "A ship change needs the ship the captain named." }
        if verb == .travel, arguments.station.trimmingCharacters(in: .whitespaces).isEmpty { return "A travel order needs the station the captain named." }
        if verb == .payLease, arguments.amount <= 0 { return "A lease payment needs the amount the captain named." }
        let order = OrderRequest(verb: verb, station: arguments.station, when: arguments.when, amount: arguments.amount > 0 ? Int64(arguments.amount) : nil,
                                 ship: arguments.ship,
                                 scope: OrderRequest.Scope(rawValue: arguments.scope) ?? .thisHull)
        voice.record([order])
        return "Order noted: \(order.sentence). Confirm it to the captain in one sentence; it files when they tap."
    }
}
#endif

// MARK: - Conversation: the captain talks to her, she answers from the journal

/// A running conversation with the ship's computer. The model keeps the turns; every
/// answer is checked against what the journal and the hull actually say before it is
/// shown or spoken — a number, id, tick or station the context never contained means the
/// answer is refused and the floor answers instead. The floor answer is the journal's own
/// lines that match the question, so the captain is never left with nothing.
public final class Conversation: @unchecked Sendable {
    public struct Turn: Equatable, Sendable {
        public var question: String
        public var answer: String
        public var lane: VoiceLane
        public var note: String?
    }

    public let voice: BridgeVoice
    public var context: BridgeContext
    public var consent: VoiceConsent
    public private(set) var turns: [Turn] = []
    private let lock = NSLock()
    #if canImport(FoundationModels)
    @available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
    private var session: LanguageModelSession? {
        get { _session as? LanguageModelSession }
        set { _session = newValue }
    }
    @available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
    private var cloudSession: LanguageModelSession? {
        get { _cloudSession as? LanguageModelSession }
        set { _cloudSession = newValue }
    }
    #endif
    private var _session: Any?
    private var _cloudSession: Any?
    /// The documents the live sessions were built with: a session binds its tools at birth,
    /// so a brief that arrived after the first question needs a fresh one.
    private var sessionDocuments: [String] = []
    /// Turns answered on the current sessions; past `turnsPerSession` they are rebuilt.
    private var turnsOnSession = 0

    /// Which lanes this conversation would try right now, and why the others would not.
    public var lanes: [VoiceLane: String] { BridgeVoice.availability(consent: consent) }

    /// Warm the on-device model before the captain's first question (time to first token is
    /// the cost the captain feels). Safe to call any time; a no-op where
    /// the model is not available.
    public func prewarm() {
        #if canImport(FoundationModels)
        if #available(macOS 26.0, iOS 26.0, visionOS 26.0, *), case .available = SystemLanguageModel.default.availability {
            let s = onDeviceSession()
            s.prewarm()
        }
        #endif
    }

    #if canImport(FoundationModels)
    @available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
    private func onDeviceSession() -> LanguageModelSession {
        lock.lock(); defer { lock.unlock() }
        let docs = context.documents.map(\.name)
        if let s = session, docs == sessionDocuments, turnsOnSession < BridgeVoice.turnsPerSession { return s }
        let s = LanguageModelSession(tools: voice.tools(context), instructions: voice.instructions(frame: context.frame, documents: context.documents))
        session = s; sessionDocuments = docs; turnsOnSession = 0
        return s
    }

    /// A session with the instructions and the last exchange only — Apple's "condensed
    /// session" for `contextSizeExceeded` (WWDC 301): the captain keeps the thread, the
    /// journal's bulk leaves the window.
    @available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
    private func condensed(_ old: LanguageModelSession, tools: [any Tool]) -> LanguageModelSession {
        let t = old.transcript
        var kept: [Transcript.Entry] = []
        if let first = t.first { kept.append(first) }
        if t.count > 2, let last = t.last { kept.append(last) }
        return LanguageModelSession(tools: tools, transcript: Transcript(entries: kept))
    }

    /// One lane's answer: the reply, or why this lane could not give one. Every error the
    /// framework names is named back, so the note says what happened rather than "error".
    @available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
    private func answer(on s: LanguageModelSession, prompt: String, lane: VoiceLane) async -> Result<String, VoiceFailure> {
        do {
            return .success(try await s.respond(to: prompt, options: BridgeVoice.focused).content)
        } catch let e as LanguageModelSession.GenerationError {
            switch e {
            case .exceededContextWindowSize:
                // Condense once and ask again; a second overflow is the floor's turn.
                let fresh = condensed(s, tools: voice.tools(context))
                if lane == .onDevice { lock.lock(); session = fresh; turnsOnSession = 0; lock.unlock() } else { lock.lock(); cloudSession = fresh; lock.unlock() }
                do { return .success(try await fresh.respond(to: prompt, options: BridgeVoice.focused).content) }
                catch { return .failure(VoiceFailure(why: "the window overflowed twice; started fresh")) }
            case .guardrailViolation: return .failure(VoiceFailure(why: "Apple's safety filter declined this one"))
            case .refusal: return .failure(VoiceFailure(why: "the model declined to answer"))
            case .rateLimited: return .failure(VoiceFailure(why: "the model is rate-limited just now"))
            case .unsupportedLanguageOrLocale: return .failure(VoiceFailure(why: "the model does not speak this locale"))
            case .assetsUnavailable: return .failure(VoiceFailure(why: "the model's assets are not on this device yet"))
            case .concurrentRequests: return .failure(VoiceFailure(why: "she was still answering the last question"))
            default: return .failure(VoiceFailure(why: "\(e)"))
            }
        } catch {
            return .failure(VoiceFailure(why: "\(error)"))
        }
    }
    #endif

    public init(voice: BridgeVoice, context: BridgeContext, consent: VoiceConsent = VoiceConsent()) {
        self.voice = voice; self.context = context; self.consent = consent
    }

    /// The floor's answer, with no model: the document the question is about, whole (a
    /// question about refuelling gets the fuel picture), else the journal lines that mention
    /// the question's words, told plainly.
    /// Words that widen a question from the hull in view to the captain's whole fleet.
    public static let fleetWords: Set<String> = ["fleet", "other", "others", "ships", "hulls", "both", "every", "pooled", "rest", "elsewhere"]

    public func floorAnswer(_ question: String) -> String {
        let floor = voice.floor(context)
        let words = question.lowercased().split(whereSeparator: { !$0.isLetter && !$0.isNumber && $0 != "-" }).map(String.init).filter { $0.count > 2 }
        // "refuel" ⊃ "fuel", "pumps" ⊃ "pump": a word matches a document when either contains the other.
        func hits(_ a: String, _ b: String) -> Bool { a.contains(b) || b.contains(a) }
        // The `fleet` document covers EVERY hull the captain flies. It answers only when the
        // captain asks about the fleet or another hull; otherwise the ship in view is the
        // subject, so a "where are we" never comes back as both hulls at once (2026-09-05:
        // Felix's dialog showed across both PROD hulls with no segregation).
        let asksFleet = words.contains { Conversation.fleetWords.contains($0) }
        let candidates = context.documents.filter { asksFleet || $0.name != "fleet" }
        let scored = candidates.map { d -> (ContextDocument, Int) in
            let vocab = (d.name + " " + d.title).lowercased().split(whereSeparator: { !$0.isLetter }).map(String.init).filter { $0.count > 3 }
            return (d, words.filter { w in vocab.contains { hits(w, $0) } }.count)
        }
        if let best = scored.max(by: { $0.1 < $1.1 }), best.1 > 0 { return best.0.text }
        let lines = floor.facts + candidates.flatMap { $0.text.split(separator: "\n").map(String.init) }
        let facts = lines.filter { f in words.contains { w in f.lowercased().split(whereSeparator: { !$0.isLetter && !$0.isNumber && $0 != "-" }).contains { hits(String($0), w) } } }
        if !facts.isEmpty { return facts.prefix(6).joined(separator: "\n") }
        return floor.headline + "\n" + floor.facts.prefix(3).joined(separator: "\n") + "\n" + floor.nextAct
    }

    /// Ask her. Always answers; the lane says who spoke.
    public func ask(_ question: String) async -> Turn {
        // AN ORDER IS NOT A QUESTION. Read deterministically first, on every lane:
        // the captain's word becomes the orders waiting for the tap, and the answer is the
        // order read back — never the journal's status, whatever the model would have said.
        if let orders = OrderParser.parse(question) {
            voice.record(orders)
            return record(Turn(question: question, answer: OrderParser.readback(orders), lane: .templated, note: nil))
        }
        let floor = voice.floor(context)
        let floorText = floorAnswer(question)
        let words = lanes
        #if canImport(FoundationModels)
        if #available(macOS 26.0, iOS 26.0, visionOS 26.0, *) {
            let docList = context.documents.isEmpty ? "" : "\nDocuments you can read: " + context.documents.map { "read_\($0.name) (\($0.title))" }.joined(separator: ", ")
            let prompt = """
            The captain says: "\(question)"
            \(context.frame.map { "Context: \($0)." } ?? "")
            FIRST decide: is this an ORDER (the captain telling you or the fleet to do something, in any words) or a QUESTION? \
            An order: call `giveOrder` for each order in it (a rendezvous is a travel AND a hold; "back to work" / "normal operations" is resume), then reply in one sentence confirming it. \
            A question: answer in two to four sentences, in your voice, from the FACTS and the documents only (every number, id, tick and station must come from them). Give the answer first, then the one thing the captain can do about it.
            FACTS:
            \(floor.facts.map { "- " + $0 }.joined(separator: "\n"))
            Hull now: \(context.hull.map { TemplatedVoice(persona: voice.persona).hullLine($0) } ?? "not on the wire")\(docList)
            Mood: \(floor.mood.rawValue). If nothing you have answers it, say what you do know and what you do not — never guess.
            """
            let truth = context.truth(floor: floor) + "\n" + (context.hull.map { TemplatedVoice(persona: voice.persona).hullLine($0) } ?? "")
            var notes: [String] = []
            // THE LADDER: Private Cloud Compute where the captain allowed it and the
            // process may call it, then the device, then the floor — the same order the report
            // already climbs; until now a QUESTION only ever had the device.
            let onDevice: Bool = { if case .available = SystemLanguageModel.default.availability { return true } else { return false } }()
            for lane in BridgeVoice.ladder(consent: consent, pccAvailable: words[.privateCloudCompute] == "available", onDeviceAvailable: onDevice) {
                let s: LanguageModelSession?
                switch lane {
                case .privateCloudCompute: s = cloudSessionIfAny()
                case .onDevice: s = onDeviceSession()
                case .templated: s = nil
                }
                guard let s else { break }
                let result = await answer(on: s, prompt: prompt, lane: lane)
                // THE MODEL'S ORDER DECIDES THE TURN. If `giveOrder` recorded anything
                // while the model read the sentence, this was an order in the captain's own
                // words: the answer is those orders read back — deterministic, never held to
                // the journal's facts (an order names a station the facts never held) — and
                // never the status the model might also have written. Reported 2026-09-20:
                // "the response is status and something about calling paws. Unacceptable."
                let given = voice.peekOrders()
                if !given.isEmpty {
                    lock.lock(); turnsOnSession += 1; lock.unlock()
                    return record(Turn(question: question, answer: OrderParser.readback(given), lane: lane, note: notes.isEmpty ? nil : notes.joined(separator: "; ")))
                }
                switch result {
                case .success(let reply):
                    lock.lock(); turnsOnSession += 1; lock.unlock()
                    if let why = Grounding.checkReply(reply, truth: truth, facts: floor.facts) {
                        notes.append("\(lane.rawValue): her answer was not the journal's (\(why))")
                        continue
                    }
                    return record(Turn(question: question, answer: reply, lane: lane, note: notes.isEmpty ? nil : notes.joined(separator: "; ")))
                case .failure(let why):
                    notes.append("\(lane.rawValue): \(why)")
                }
            }
            if !onDevice { notes.append("on-device: \(words[.onDevice] ?? "unavailable")") }
            return record(Turn(question: question, answer: floorText, lane: .templated, note: notes.joined(separator: "; ") + "; the journal's own words instead"))
        }
        #endif
        return record(Turn(question: question, answer: floorText, lane: .templated, note: "on-device: \(words[.onDevice] ?? "unavailable")"))
    }

    #if canImport(FoundationModels)
    /// The PCC session, only where the 27 SDK built it, the captain allowed it and the
    /// process is entitled; nil otherwise, and the ladder moves on.
    @available(macOS 26.0, iOS 26.0, visionOS 26.0, *)
    private func cloudSessionIfAny() -> LanguageModelSession? {
        guard consent.mayUsePrivateCloudCompute else { return nil }
        #if compiler(>=6.4)
        if #available(macOS 27.0, iOS 27.0, visionOS 27.0, *) {
            lock.lock(); defer { lock.unlock() }
            let docs = context.documents.map(\.name)
            if let s = cloudSession, docs == sessionDocuments, turnsOnSession < BridgeVoice.turnsPerSession { return s }
            let pcc = PrivateCloudComputeLanguageModel()
            guard case .available = pcc.availability else { return nil }
            let s = LanguageModelSession(model: pcc, tools: voice.tools(context), instructions: voice.instructions(frame: context.frame, documents: context.documents))
            cloudSession = s
            return s
        }
        #endif
        return nil
    }
    #endif

    private func record(_ t: Turn) -> Turn {
        lock.lock(); defer { lock.unlock() }
        turns.append(t)
        return t
    }

    public func reset() { lock.lock(); turns.removeAll(); _session = nil; _cloudSession = nil; sessionDocuments = []; turnsOnSession = 0; lock.unlock() }
}
