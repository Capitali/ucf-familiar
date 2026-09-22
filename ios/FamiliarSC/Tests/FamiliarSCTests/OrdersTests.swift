import XCTest
@testable import FamiliarSC
@testable import FamiliarSCUI

/// T-252: the captain's word is an order. Ian's exact sentence reads as a travel and a hold
/// for every hull; the conversation answers with the order read back, never status; the
/// routes are the host's; a question is still a question.
final class OrdersTests: XCTestCase {
    func testIansSentenceIsATravelAndAHoldForTheFleet() throws {
        let orders = try XCTUnwrap(OrderParser.parse("Felix, bring all the ships to paws truck stop. Wait there for my next instruction."))
        XCTAssertEqual(orders.map(\.verb), [.travel, .hold])
        XCTAssertEqual(orders.map(\.scope), [.fleet, .fleet])
        XCTAssertEqual(orders[0].station, "paws truck stop")
        XCTAssertEqual(orders[1].station, "paws truck stop", "there = where the travel goes")
        XCTAssertEqual(orders[0].when, "now")
        XCTAssertEqual(orders[0].body, ["verb": .string("travel"), "when": .string("now"), "station": .string("paws truck stop")])
        let said = OrderParser.readback(orders)
        XCTAssertTrue(said.hasPrefix("Order read — every hull: travel to paws truck stop; every hull: hold at paws truck stop until your next word."), said)
        XCTAssertTrue(said.contains("Tap FILE"), said)
        // The earlier, shorter form on the same screen.
        let short = try XCTUnwrap(OrderParser.parse("Bring lol ships to the paws truck stop"))
        XCTAssertEqual(short.map(\.verb), [.travel]); XCTAssertEqual(short[0].station, "paws truck stop"); XCTAssertEqual(short[0].scope, .thisHull, "'lol ships' is not 'all ships'; the fleet needs saying")
    }

    /// 2026-09-20: "rendezvous at tuna-prime" filed a travel and no hold, so the first hull to
    /// arrive left again. A rendezvous is a travel and a hold, for the fleet.
    func testARendezvousIsATravelAndAHoldForTheFleet() throws {
        let orders = try XCTUnwrap(OrderParser.parse("Felix, rendezvous at tuna prime"))
        XCTAssertEqual(orders.map(\.verb), [.travel, .hold])
        XCTAssertEqual(orders.map(\.scope), [.fleet, .fleet])
        XCTAssertEqual(orders.map(\.station), ["tuna prime", "tuna prime"])
        XCTAssertEqual(OrderParser.parse("all ships meet at the paws truck stop")?.map(\.verb), [.travel, .hold])
        XCTAssertEqual(OrderParser.parse("gather the fleet at foxy's diner and wait")?.map(\.verb), [.travel, .hold, .hold].prefix(2).map { $0 }, "the explicit wait adds nothing new")
        XCTAssertEqual(OrderParser.parse("regroup this ship at cannery row")?.map(\.scope), [.thisHull, .thisHull])
    }

    /// T-254 (Ian, 2026-09-20): the four sentences that must at least reach the floor's reader.
    func testTheCaptainsOwnPhrasingsReachTheFloor() throws {
        XCTAssertEqual(OrderParser.parse("everyone get to tuna-prime")?.map { "\($0.verb.rawValue) \($0.station ?? "") \($0.scope.rawValue)" }, ["travel tuna-prime fleet"])
        XCTAssertEqual(OrderParser.parse("All ships head to tuna-prime")?.map { "\($0.verb.rawValue) \($0.station ?? "") \($0.scope.rawValue)" }, ["travel tuna-prime fleet"])
        XCTAssertEqual(OrderParser.parse("command all ships to rendezvous at tuna prime")?.map { "\($0.verb.rawValue) \($0.scope.rawValue)" }, ["travel fleet", "hold fleet"])
        XCTAssertEqual(OrderParser.parse("all ships return to normal operations")?.map { "\($0.verb.rawValue) \($0.scope.rawValue)" }, ["resume fleet"], "normal operations is a resume, never a travel to a station called 'normal operations'")
        XCTAssertEqual(OrderParser.parse("Everyone back to work")?.map { "\($0.verb.rawValue) \($0.scope.rawValue)" }, ["resume fleet"])
        XCTAssertEqual(OrderParser.parse("Felix, stand down and fly as you see fit")?.first?.verb, .resume)
    }

    /// T-254: what the model's tool records decides the turn — the read-back of the recorded
    /// orders, on the model's lane, never a status answer.
    func testARecordedOrderIsReadBackNotGrounded() {
        let voice = BridgeVoice(persona: Persona(name: "Felix"))
        voice.record([OrderRequest(verb: .travel, station: "tuna-prime", scope: .fleet)])
        let peeked = voice.peekOrders()
        XCTAssertEqual(peeked.count, 1, "peek leaves the order in place")
        XCTAssertTrue(OrderParser.readback(peeked).hasPrefix("Order read — every hull: travel to tuna-prime"))
        XCTAssertEqual(voice.takeOrders().count, 1, "take still hands it to the app once")
        XCTAssertTrue(voice.peekOrders().isEmpty)
    }

    func testTheOtherThingsACaptainSays() throws {
        XCTAssertEqual(OrderParser.parse("go to cannery row")?.map { "\($0.verb.rawValue) \($0.station ?? "") \($0.scope.rawValue)" }, ["travel cannery row this-hull"])
        XCTAssertEqual(OrderParser.parse("take the fleet to tuna prime and wait")?.map(\.verb), [.travel, .hold])
        XCTAssertEqual(OrderParser.parse("Felix, hold here")?.first.map { ($0.verb, $0.station) }.map { "\($0.0) \($0.1 ?? "-")" }, "hold -")
        XCTAssertEqual(OrderParser.parse("repair at next docking")?.first?.when, "next-docking")
        XCTAssertEqual(OrderParser.parse("repair now")?.first?.when, "now")
        let fuel = try XCTUnwrap(OrderParser.parse("refuel 200 units now")?.first)
        XCTAssertEqual((fuel.verb, fuel.amount, fuel.when).0, .refuel); XCTAssertEqual(fuel.amount, 200); XCTAssertEqual(fuel.when, "now")
        let pay = try XCTUnwrap(OrderParser.parse("pay 5,000 down on the lease")?.first)
        XCTAssertEqual(pay.verb, .payLease); XCTAssertEqual(pay.amount, 5000)
        XCTAssertEqual(OrderParser.parse("every ship to foxys diner, then hold there")?.map(\.sentence), ["every hull: travel to foxys diner", "every hull: hold at foxys diner until your next word"])
        // The override words: the hold comes off.
        XCTAssertEqual(OrderParser.parse("Felix, as you were")?.map(\.verb), [.resume])
        XCTAssertEqual(OrderParser.parse("resume, all ships")?.map { "\($0.verb.rawValue) \($0.scope.rawValue)" }, ["resume fleet"])
        XCTAssertEqual(OrderParser.parse("cancel the hold and carry on")?.map(\.verb), [.resume])
        XCTAssertTrue(OrderParser.readback([OrderRequest(verb: .resume, scope: .fleet)]).contains("the standing course is lifted"))
        // The tanker (Ian's second screenshot: "Allow the pilot to,call paws." got status).
        XCTAssertEqual(OrderParser.parse("Allow the pilot to,call paws.")?.map(\.verb), [.callPaws])
        XCTAssertEqual(OrderParser.parse("Felix, call paws")?.first?.body["verb"], .string("paws"))
        XCTAssertEqual(OrderParser.parse("send for the tanker now")?.first?.when, "now")
        XCTAssertNil(OrderParser.parse("do not call paws"))
        XCTAssertTrue(OrderParser.readback([OrderRequest(verb: .callPaws)]).contains("calls the tanker under your authority"))
        // Questions and talk are not orders.
        XCTAssertNil(OrderParser.parse("Where are we going?"))
        XCTAssertNil(OrderParser.parse("How much fuel do we have"))
        XCTAssertNil(OrderParser.parse("What did you do today?"))
        XCTAssertNil(OrderParser.parse("Do not wait for me?"))
        XCTAssertNil(OrderParser.parse("pay attention to the lease"), "no amount, no payment")
    }

    func testTheConversationAnswersAnOrderWithTheOrderNotTheJournal() async throws {
        let voice = BridgeVoice(persona: Persona(name: "Felix", style: nil))
        let ctx = BridgeContext(entries: [], hull: nil, openProposals: 0, frame: "ship, hull Kibble Klipper (PROD)", documents: [])
        let c = Conversation(voice: voice, context: ctx)
        let turn = await c.ask("Felix, bring all the ships to paws truck stop. Wait there for my next instruction.")
        XCTAssertTrue(turn.answer.hasPrefix("Order read — "), turn.answer)
        XCTAssertFalse(turn.answer.lowercased().contains("distress"), turn.answer)
        XCTAssertNil(turn.note)
        let orders = voice.takeOrders()
        XCTAssertEqual(orders.map(\.verb), [.travel, .hold])
        XCTAssertTrue(voice.takeOrders().isEmpty, "taken once, they belong to the app")
    }

    func testTheRoutesAreTheHosts() {
        XCTAssertEqual(WireFeed.captainOrdersPath(briefPath: "captains/c_9f3/brief"), "captains/c_9f3/orders")
        XCTAssertNil(WireFeed.captainOrdersPath(briefPath: "ships/w/brief"))
        let hull = OrderRequest(verb: .repair, scope: .thisHull)
        XCTAssertEqual(hull.body["when"], .string("next-docking"))
        XCTAssertEqual(OrderRequest(verb: .hold, scope: .fleet).sentence, "every hull: hold where it is until your next word")
    }
}
