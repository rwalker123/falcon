//! **Seats — who is allowed to drive a faction, and how long a turn waits for them.**
//!
//! > The sim knows seats. It never knows who fills one.
//!
//! A world has N faction seats ([`crate::FactionRegistry`]). A seat is **occupied** by whatever
//! command-socket connection has claimed it, or **vacant**. That sentence is the simulation's entire
//! model of "who is playing": there is no AI path and no human path, only a socket
//! (`docs/plan_multiplayer_seats.md` §1).
//!
//! **Everything here is session state.** A save is a world with N seats; who sat in them is a fact
//! about this process's sockets and is deliberately not in `SimState` (plan §4.5). Nothing in this
//! module is a `Resource`, so the server binary owns a [`SeatRegistry`] beside its `world_active` and
//! `CommandLog` locals and a checkpoint cannot pick it up by accident.
//!
//! See `.claude/rules/core_sim/factions.md` → "Seats: a connection claims the faction it drives".

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use rand::Rng;
use sim_runtime::commands::{seat_error, NO_SEAT_TOKEN};

use crate::orders::FactionId;

/// **One accepted command socket's opaque identity**, minted by [`ConnectionIdAllocator`].
///
/// Opaque on purpose: it says *which connection*, and nothing about who is on the other end. It is
/// the value the **log** names a connection by (`connection=5`), which is why it stays a short
/// sequential counter — the secret its stream socket presents is a separate [`SeatToken`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId(pub u64);

impl ConnectionId {
    /// **The server's own voice.** Config-file watchers and the in-process command sender speak on
    /// this id; the allocator never mints it, so it can never collide with a socket.
    ///
    /// It holds no seat, which is exactly right for what it sends: world verbs that name no faction.
    pub const INTERNAL: Self = Self(0);
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Mints one [`ConnectionId`] per accepted socket. Shared by the accept thread, hence the atomic.
#[derive(Debug)]
pub struct ConnectionIdAllocator(AtomicU64);

impl ConnectionIdAllocator {
    /// Starts one past [`ConnectionId::INTERNAL`], so no socket is ever handed the server's own id.
    pub fn new() -> Self {
        Self(AtomicU64::new(ConnectionId::INTERNAL.0 + 1))
    }

    pub fn mint(&self) -> ConnectionId {
        ConnectionId(self.0.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for ConnectionIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// **The secret a seat's STREAM socket presents to be sent that seat's frames**, minted fresh by
/// every granted claim and deliberately *not* the claimant's [`ConnectionId`].
///
/// The pair is **identity versus secret**, and one value cannot be both:
///
/// - a [`ConnectionId`] is what a human reads while debugging a session (`connection=5`,
///   `command.rejected=… connection=7`), so it wants to be short and sequential;
/// - a seat token is what entitles its bearer to a seat's private world, so it wants to resist
///   guessing — and a sequential one is enumerable by anyone who can open the snapshot socket.
///
/// Separating them is what lets the identity stay in the log while the secret stays out of it:
/// ⛔ **a seat token is never logged, whole or in part.** The connection that holds the seat is
/// already in the log (`seat.claimed`), and that is the half a human wants.
///
/// ⛔ **It is session state, and no part of the simulation may see it.** It is drawn from the OS
/// entropy pool via [`rand::thread_rng`] — never from a sim RNG, never seeded from `map_seed` — so a
/// determinism suite cannot observe it and a replay cannot reproduce it. It reaches `SimState`, the
/// command log and a published frame nowhere at all; the only two places it appears are the claim
/// reply that mints it and the greeting that presents it back.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeatToken(u64);

impl SeatToken {
    /// **No token.** What a refused claim hands back, what a stream connection holds until it greets,
    /// and what a peer presenting nothing falls back to.
    ///
    /// Spelled as the wire constant the client also reads, so the sentinel the server means and the
    /// sentinel the client tests for cannot drift apart.
    pub const NONE: Self = Self(NO_SEAT_TOKEN);

    /// **Mint one from a cryptographically strong source.**
    ///
    /// `thread_rng` is `rand`'s CSPRNG, seeded from the OS: the value is not merely unlikely to
    /// repeat but unpredictable from any other token. It is drawn from the range **strictly above
    /// [`Self::NONE`]**, which is how the sentinel is excluded — by construction, rather than by a
    /// retry loop guarding a 1-in-2^64 draw.
    ///
    /// A collision with a live token is not checked either: over a `u64` the birthday bound is many
    /// orders of magnitude beyond the number of seats a session claims, so the check would guard an
    /// event that cannot be reached.
    pub fn mint() -> Self {
        Self(rand::thread_rng().gen_range(Self::NONE.0 + 1..=u64::MAX))
    }

    /// The value the claim reply carries, and the value the greeting presents back little-endian.
    pub fn wire(self) -> u64 {
        self.0
    }

    /// **Read a presented greeting.** `NO_SEAT_TOKEN` is [`Self::NONE`] — the explicit "I hold no
    /// seat" — and any other value is resolved against the live claims at *delivery*, so a wrong or
    /// guessed one simply names no seat and is sent nothing.
    pub const fn from_wire(bits: u64) -> Self {
        Self(bits)
    }
}

/// ⛔ **Redacted on purpose.** The one formatter a token has says nothing about its value, so a
/// `{:?}` reached for in a hurry — or a `#[derive(Debug)]` on a struct that holds one — cannot put
/// the secret in a log line. There is deliberately no `Display`, so `%token` in a `tracing` field or
/// a `{token}` in a `log::` string does not compile.
impl fmt::Debug for SeatToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SeatToken(redacted)")
    }
}

/// **What a granted claim recorded**: which connection holds the seat, and the token its stream
/// socket must present. Two values because they answer two different questions — see [`SeatToken`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatClaimant {
    pub connection: ConnectionId,
    pub token: SeatToken,
}

/// Why a [`SeatRegistry::claim`] was refused. Each maps to a wire token
/// (`sim_runtime::commands::seat_error`) the client turns into prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatClaimRefusal {
    /// The faction id is in no seat of this world's roster.
    UnknownSeat,
    /// Another live connection holds it. **First claim wins** — there is no takeover, because there
    /// is no credential that would make one safe (plan §4.1: claiming is not authentication).
    SeatOccupied,
    /// This connection already holds a seat. One seat per connection, so even re-claiming the seat it
    /// already holds is refused rather than silently moved.
    AlreadySeated,
}

impl SeatClaimRefusal {
    /// The machine-readable token the reply carries — **taken from `sim_runtime`'s constants rather
    /// than spelled again here**, so the refusal the client matches on and the refusal the server
    /// raises cannot drift apart.
    pub fn token(self) -> &'static str {
        match self {
            Self::UnknownSeat => seat_error::UNKNOWN_SEAT,
            Self::SeatOccupied => seat_error::SEAT_OCCUPIED,
            Self::AlreadySeated => seat_error::ALREADY_SEATED,
        }
    }
}

/// **Which connection drives which faction.** The whole seat model.
///
/// The map is kept bijective — one connection, one seat — which is what lets both directions be
/// answered without a second index: [`Self::claimant_of`] is a lookup and [`Self::seat_of`] is a
/// scan over at most the roster.
#[derive(Debug, Default)]
pub struct SeatRegistry {
    claims: BTreeMap<FactionId, SeatClaimant>,
}

impl SeatRegistry {
    /// **Seat `connection` at `seat`, or say why not.** `roster` is the world's registered factions
    /// ([`crate::FactionRegistry::factions`]); a seat outside it is refused rather than created,
    /// because a seat is a property of the world and not of the request.
    ///
    /// A grant returns **a freshly minted [`SeatToken`]** — the secret the claimant's stream socket
    /// presents. It is minted per claim rather than per connection so that a seat released and
    /// re-claimed, even by the same connection, is a new secret: the previous holder's token names
    /// nothing from the release onward.
    pub fn claim(
        &mut self,
        seat: FactionId,
        connection: ConnectionId,
        roster: &[FactionId],
    ) -> Result<SeatToken, SeatClaimRefusal> {
        if !roster.contains(&seat) {
            return Err(SeatClaimRefusal::UnknownSeat);
        }
        if self.seat_of(connection).is_some() {
            return Err(SeatClaimRefusal::AlreadySeated);
        }
        if self.claims.contains_key(&seat) {
            return Err(SeatClaimRefusal::SeatOccupied);
        }
        let token = SeatToken::mint();
        self.claims.insert(seat, SeatClaimant { connection, token });
        Ok(token)
    }

    /// The seat this connection claimed, if any.
    pub fn seat_of(&self, connection: ConnectionId) -> Option<FactionId> {
        self.claims
            .iter()
            .find(|(_, claimant)| claimant.connection == connection)
            .map(|(seat, _)| *seat)
    }

    pub fn claimant_of(&self, seat: FactionId) -> Option<ConnectionId> {
        self.claims.get(&seat).map(|claimant| claimant.connection)
    }

    /// The token the connection holding `seat` was handed. For the delivery table and for a test that
    /// has to present what a claim minted; nothing logs it.
    pub fn token_of(&self, seat: FactionId) -> Option<SeatToken> {
        self.claims.get(&seat).map(|claimant| claimant.token)
    }

    pub fn is_occupied(&self, seat: FactionId) -> bool {
        self.claims.contains_key(&seat)
    }

    /// Every occupied seat, in id order.
    pub fn occupied_seats(&self) -> Vec<FactionId> {
        self.claims.keys().copied().collect()
    }

    /// **Is there another player in this world?** More than one seat occupied.
    ///
    /// A predicate rather than a count comparison at the call site, because it is the *meaning* that
    /// the one rule keyed on it needs: `SetFogEnabled` is a convenience while there is nobody to
    /// disclose to and a disclosure switch the moment there is (`server.rs`'s `solo_only_verb`).
    pub fn is_shared(&self) -> bool {
        self.claims.len() > 1
    }

    /// Every claimant, in seat-id order — who a world-wide event has to reach, and the id its log
    /// lines name.
    pub fn claimants(&self) -> Vec<(FactionId, ConnectionId)> {
        self.claims
            .iter()
            .map(|(seat, claimant)| (*seat, claimant.connection))
            .collect()
    }

    /// **Every occupied seat with the token that reaches it**, in seat-id order — the table
    /// `network::SnapshotServer::set_seats` resolves a greeting against. Deliberately the *tokens*
    /// and not the connection ids: the stream socket knows only the secret it was handed, and it is
    /// the only thing that socket ever presents.
    pub fn delivery_tokens(&self) -> Vec<(FactionId, SeatToken)> {
        self.claims
            .iter()
            .map(|(seat, claimant)| (*seat, claimant.token))
            .collect()
    }

    /// **Free the seat a closing connection held**, returning it. What makes "occupied" honest: a
    /// player whose process died can reconnect and claim again, and without this the first crash
    /// would wedge the seat for the rest of the session.
    pub fn release(&mut self, connection: ConnectionId) -> Option<FactionId> {
        let seat = self.seat_of(connection)?;
        self.claims.remove(&seat);
        Some(seat)
    }

    /// **Drop the claims a world rebuild left behind**, returning them. `new_game`, `reset_map` and a
    /// load all replace the roster, and a claim on a faction the new world does not have would both
    /// gate nothing (the membership check refuses it anyway) and keep a live seat id unclaimable.
    pub fn retain_seats(&mut self, roster: &[FactionId]) -> Vec<(FactionId, ConnectionId)> {
        let dropped: Vec<(FactionId, ConnectionId)> = self
            .claims
            .iter()
            .filter(|(seat, _)| !roster.contains(seat))
            .map(|(seat, claimant)| (*seat, claimant.connection))
            .collect();
        for (seat, _) in &dropped {
            self.claims.remove(seat);
        }
        dropped
    }

    /// **May this connection issue a command on `faction`'s behalf?** True only for the connection
    /// holding that seat — so an unseated connection may send nothing faction-bearing at all, and a
    /// seated one may speak for its own faction and no other.
    pub fn commands_faction(&self, connection: ConnectionId, faction: FactionId) -> bool {
        self.claimant_of(faction) == Some(connection)
    }

    /// **May this connection issue a host verb?** The host verbs rewind or advance the world for
    /// *everyone* (`Rollback`, `Turn`), so a seat must not have them: with one player `Rollback` is a
    /// debug tool and with several it is a grief vector (plan §4.4).
    ///
    /// "Host" is defined here as **holding no seat** — the operator channel: the Inspector, the CLI,
    /// and the server's own [`ConnectionId::INTERNAL`] voice. That is as much authority as this phase
    /// can express, claiming being explicitly not authentication; what it buys is the property the
    /// plan asks for, that a *player* cannot rewind the world other players are in.
    pub fn may_issue_host_verb(&self, connection: ConnectionId) -> bool {
        self.seat_of(connection).is_none()
    }
}

/// **The clock levers of the live turn loop**, as a struct rather than bare constants so a test can
/// shrink them — the same arrangement, and the same reason, as [`crate::network::SnapshotServerLimits`]:
/// the behaviour worth pinning is "what happens once the wait runs out", and waiting out the shipped
/// value would make the suite minutes long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatTurnLimits {
    /// How long an open turn waits for an occupied seat that has gone silent before submitting
    /// `end_turn` on its behalf. Read from `simulation_config.json`'s `seat_turn_timeout_seconds`.
    pub submission_timeout: Duration,
}

/// What the live loop should do about the open turn. Produced by [`SeatTurnGate::assess`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnWait {
    /// **Nobody is waiting on anybody.** No occupied seat has submitted for this turn, so there is
    /// no turn in flight to hold up: the loop blocks indefinitely on the command channel.
    ///
    /// This is the shipped single-player case in full — with no seat claimed at all, a turn only ever
    /// advances because the host asked for one (`Command::Turn`), exactly as before seats existed.
    Idle,
    /// Every occupied seat has submitted. Resolve now, auto-submitting the vacant remainder.
    Resolve,
    /// The wait ran out. Resolve anyway; `silent_seats` are the occupied seats being submitted for.
    ResolveOnTimeout { silent_seats: Vec<FactionId> },
    /// Still waiting on `silent_seats`. Wake at `deadline` even if no command arrives.
    Wait {
        silent_seats: Vec<FactionId>,
        deadline: Instant,
    },
}

/// **The live loop's turn-waiting decision, and the one deadline behind it.**
///
/// ⛔ **The timeout is a live-path SCHEDULING decision and nothing else.** It never enters
/// `resolve_turn_with_auto_orders`, which is also the replay path and keeps its unconditional
/// force-submit: a real seat's submission is a logged `Command::Orders`, so a replayed
/// `LogEntry::Turn` finds that faction already submitted and the force-submit catches exactly the
/// seats the original run auto-submitted. **Replay is correct by construction rather than by
/// reproducing a timer**, and nothing here consults a wall clock on a replay because nothing here
/// runs on one.
#[derive(Debug, Default)]
pub struct SeatTurnGate {
    /// Set on the first [`TurnWait::Wait`] of a turn and cleared when the turn resolves, so the
    /// deadline is measured **once per turn from the first occupied seat's submission** rather than
    /// pushed out by every later command.
    armed: Option<Instant>,
}

impl SeatTurnGate {
    /// When the loop must wake even if no command arrives, or `None` to block.
    pub fn deadline(&self) -> Option<Instant> {
        self.armed
    }

    /// Stop waking on a deadline. For the caller that has decided there is no turn to settle at all —
    /// a wake it would answer with nothing is a spin on an expired deadline, not a wait.
    pub fn disarm(&mut self) {
        self.armed = None;
    }

    /// **The decision.** `awaiting` is [`crate::TurnQueue::awaiting`] — every faction that has not
    /// submitted, control-blind, which is why the queue itself needs no seat knowledge.
    ///
    /// Two rules produce every arm:
    ///
    /// - **A vacant seat never holds the turn.** It is not in the wait set, and the resolve
    ///   auto-submits it. That is what makes a single-human game with N vacant rivals behave exactly
    ///   as today.
    /// - **A turn is only in flight once an occupied seat has submitted.** Without that, a connected
    ///   client that never sends orders would have the server resolving turns on a timer underneath
    ///   it, and a server with no seats claimed would resolve them in a hot loop. "Somebody is
    ///   waiting" is the whole justification for the timeout, so it is also its precondition.
    ///
    /// `limits` is passed rather than held so the value is read from the **live**
    /// `SimulationConfig` on every decision — a hot reload of the file moves the wait — and so a test
    /// injects a short one by passing it.
    pub fn assess(
        &mut self,
        awaiting: &[FactionId],
        seats: &SeatRegistry,
        now: Instant,
        limits: SeatTurnLimits,
    ) -> TurnWait {
        let occupied = seats.occupied_seats();
        let silent_seats: Vec<FactionId> = occupied
            .iter()
            .copied()
            .filter(|seat| awaiting.contains(seat))
            .collect();
        // Nobody is waiting: either no seat is occupied, or no occupied seat has answered yet.
        if occupied.is_empty() || silent_seats.len() == occupied.len() {
            self.armed = None;
            return TurnWait::Idle;
        }
        if silent_seats.is_empty() {
            self.armed = None;
            return TurnWait::Resolve;
        }
        let deadline = *self
            .armed
            .get_or_insert_with(|| now + limits.submission_timeout);
        if now >= deadline {
            self.armed = None;
            return TurnWait::ResolveOnTimeout { silent_seats };
        }
        TurnWait::Wait {
            silent_seats,
            deadline,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: FactionId = FactionId(0);
    const RIVAL: FactionId = FactionId(1);
    const ROSTER: [FactionId; 2] = [HOME, RIVAL];

    const FIRST: ConnectionId = ConnectionId(1);
    const SECOND: ConnectionId = ConnectionId(2);

    fn limits() -> SeatTurnLimits {
        SeatTurnLimits {
            submission_timeout: Duration::from_millis(50),
        }
    }

    #[test]
    fn the_allocator_never_mints_the_servers_own_id() {
        let allocator = ConnectionIdAllocator::new();
        let minted: Vec<ConnectionId> = (0..3).map(|_| allocator.mint()).collect();
        assert!(!minted.contains(&ConnectionId::INTERNAL));
        assert_eq!(minted[0], FIRST, "ids start one past the internal voice");
        assert!(
            minted[1] != minted[0] && minted[2] != minted[1],
            "every socket gets its own id"
        );
    }

    #[test]
    fn a_claim_seats_the_connection_both_ways_round() {
        let mut seats = SeatRegistry::default();
        let token = seats.claim(HOME, FIRST, &ROSTER).expect("the claim");
        assert_eq!(seats.seat_of(FIRST), Some(HOME));
        assert_eq!(seats.claimant_of(HOME), Some(FIRST));
        assert_eq!(
            seats.token_of(HOME),
            Some(token),
            "the token the claimant was handed is the token the delivery table holds"
        );
        assert_eq!(
            seats.delivery_tokens(),
            vec![(HOME, token)],
            "and the occupied seat is reachable by it and by nothing else"
        );
        assert!(seats.is_occupied(HOME));
        assert!(!seats.is_occupied(RIVAL), "the rival seat stays vacant");
    }

    /// **First claim wins, and the second is told why.** The refusal is explicit rather than a
    /// silent re-seat: two clients each believing they hold a seat is the failure this prevents.
    #[test]
    fn a_second_claim_on_an_occupied_seat_is_refused() {
        let mut seats = SeatRegistry::default();
        seats.claim(HOME, FIRST, &ROSTER).expect("the first claim");
        assert_eq!(
            seats.claim(HOME, SECOND, &ROSTER),
            Err(SeatClaimRefusal::SeatOccupied)
        );
        assert_eq!(
            seats.claimant_of(HOME),
            Some(FIRST),
            "the sitting connection keeps the seat"
        );
    }

    #[test]
    fn one_seat_per_connection_and_a_seat_outside_the_roster_is_not_a_seat() {
        let mut seats = SeatRegistry::default();
        seats.claim(HOME, FIRST, &ROSTER).expect("the first claim");
        assert_eq!(
            seats.claim(RIVAL, FIRST, &ROSTER),
            Err(SeatClaimRefusal::AlreadySeated)
        );
        assert_eq!(
            seats.claim(FactionId(7), SECOND, &ROSTER),
            Err(SeatClaimRefusal::UnknownSeat)
        );
    }

    #[test]
    fn a_closed_connection_frees_its_seat_for_the_next_claimant() {
        let mut seats = SeatRegistry::default();
        let first_token = seats.claim(HOME, FIRST, &ROSTER).expect("the first claim");
        assert_eq!(seats.release(FIRST), Some(HOME));
        assert_eq!(seats.release(FIRST), None, "releasing twice is a no-op");
        let second_token = seats
            .claim(HOME, SECOND, &ROSTER)
            .expect("the next claimant");
        assert_ne!(
            first_token, second_token,
            "a re-claimed seat is a NEW secret: the previous holder's token must name nothing, or a \
             stream socket it left open would keep receiving the next occupant's world"
        );
    }

    /// ⛔ **A TOKEN IS A SECRET, SO IT IS RANDOM AND IT IS NEVER THE SENTINEL.**
    ///
    /// The whole point of minting it apart from [`ConnectionId`]: the ids the log names are 1, 2, 3
    /// and are therefore guessable, so the value that entitles a stream socket to a seat's private
    /// world cannot be one of them. And `0` means *"no token"* everywhere downstream
    /// (`SeatToken::NONE`, the greeting's fallback, a refusal's reply), so a minted one that landed
    /// on it would read as unseated.
    #[test]
    fn every_claim_mints_its_own_token_and_never_the_no_token_sentinel() {
        /// Draws taken per seat. Small — the sentinel is excluded by the mint's range rather than by
        /// chance, so this asserts the property holds repeatedly, not that a rare draw is unlikely.
        const DRAWS: usize = 32;

        let mut minted = Vec::with_capacity(DRAWS * ROSTER.len());
        for draw in 0..DRAWS {
            let mut seats = SeatRegistry::default();
            minted.push(seats.claim(HOME, FIRST, &ROSTER).expect("home"));
            minted.push(seats.claim(RIVAL, SECOND, &ROSTER).expect("rival"));
            assert_ne!(
                minted[draw * ROSTER.len()],
                minted[draw * ROSTER.len() + 1],
                "two live seats must not share a token"
            );
        }
        assert!(
            !minted.contains(&SeatToken::NONE),
            "a minted token must never be the no-token sentinel"
        );
        assert!(
            !minted
                .iter()
                .any(|token| token.wire() == FIRST.0 || token.wire() == SECOND.0),
            "a minted token is not the claiming connection's id — the ids a counter hands out are \
             exactly what a guess would try"
        );
        minted.sort_unstable();
        let distinct = {
            let mut distinct = minted.clone();
            distinct.dedup();
            distinct.len()
        };
        assert_eq!(distinct, minted.len(), "every mint is its own value");
    }

    /// ⛔ **THE TOKEN HAS NO FORMATTER THAT SHOWS IT.** `Debug` is the only one, and it is redacted,
    /// so a `{:?}` on a token — or on a `SeatRegistry` that holds one — cannot leak the secret into a
    /// log line. There is no `Display`, which is what stops `%token` compiling in a `tracing` field.
    #[test]
    fn a_token_never_formats_its_value() {
        let mut seats = SeatRegistry::default();
        let token = seats.claim(HOME, FIRST, &ROSTER).expect("the claim");
        let value = token.wire().to_string();
        for rendered in [format!("{token:?}"), format!("{seats:?}")] {
            assert!(
                !rendered.contains(&value),
                "a formatter rendered the token's value: {rendered}"
            );
        }
    }

    #[test]
    fn a_world_rebuild_drops_the_claims_its_roster_no_longer_has() {
        let mut seats = SeatRegistry::default();
        seats.claim(HOME, FIRST, &ROSTER).expect("home");
        seats.claim(RIVAL, SECOND, &ROSTER).expect("rival");
        assert_eq!(seats.retain_seats(&[HOME]), vec![(RIVAL, SECOND)]);
        assert_eq!(seats.seat_of(SECOND), None);
        assert_eq!(seats.seat_of(FIRST), Some(HOME), "the surviving seat stays");
    }

    #[test]
    fn only_the_claimant_commands_a_faction_and_only_the_unseated_may_host() {
        let mut seats = SeatRegistry::default();
        seats.claim(HOME, FIRST, &ROSTER).expect("the claim");
        assert!(seats.commands_faction(FIRST, HOME));
        assert!(
            !seats.commands_faction(FIRST, RIVAL),
            "a seat commands its own faction and no other"
        );
        assert!(
            !seats.commands_faction(SECOND, HOME),
            "and an unseated connection commands nothing"
        );
        assert!(
            !seats.may_issue_host_verb(FIRST),
            "a player is not the host"
        );
        assert!(seats.may_issue_host_verb(SECOND));
        assert!(seats.may_issue_host_verb(ConnectionId::INTERNAL));
    }

    /// ⛔ **THE SHIPPED SINGLE-PLAYER CASE: no seat claimed, so no turn is ever in flight.** The
    /// gate must never resolve a turn on its own, or a server nobody has sent orders to would march
    /// the world forward under the player.
    #[test]
    fn with_no_seat_claimed_the_gate_is_idle_and_arms_no_deadline() {
        let mut gate = SeatTurnGate::default();
        let seats = SeatRegistry::default();
        assert_eq!(
            gate.assess(&ROSTER, &seats, Instant::now(), limits()),
            TurnWait::Idle
        );
        assert_eq!(gate.deadline(), None);
        // And with nothing awaited either — the queue between a resolve and the next turn.
        assert_eq!(
            gate.assess(&[], &seats, Instant::now(), limits()),
            TurnWait::Idle
        );
        assert_eq!(gate.deadline(), None);
    }

    /// **A vacant rival seat never holds the turn**: the one occupied seat submits and the turn
    /// resolves immediately, which is today's pacing exactly.
    #[test]
    fn a_vacant_seat_does_not_hold_the_turn() {
        let mut gate = SeatTurnGate::default();
        let mut seats = SeatRegistry::default();
        seats.claim(HOME, FIRST, &ROSTER).expect("the claim");

        // HOME still awaited: nobody has submitted, so nothing is in flight.
        assert_eq!(
            gate.assess(&ROSTER, &seats, Instant::now(), limits()),
            TurnWait::Idle
        );
        // HOME has submitted; RIVAL is vacant and awaited.
        assert_eq!(
            gate.assess(&[RIVAL], &seats, Instant::now(), limits()),
            TurnWait::Resolve
        );
        assert_eq!(gate.deadline(), None, "a resolve leaves nothing armed");
    }

    /// **An occupied seat is waited for, and auto-submitted once the wait runs out.** Both halves in
    /// one test because the wait is only meaningful against the resolve that eventually happens.
    #[test]
    fn an_occupied_seat_is_waited_for_until_the_timeout() {
        let mut gate = SeatTurnGate::default();
        let mut seats = SeatRegistry::default();
        seats.claim(HOME, FIRST, &ROSTER).expect("home");
        seats.claim(RIVAL, SECOND, &ROSTER).expect("rival");

        let opened = Instant::now();
        assert_eq!(
            gate.assess(&[RIVAL], &seats, opened, limits()),
            TurnWait::Wait {
                silent_seats: vec![RIVAL],
                deadline: opened + limits().submission_timeout,
            },
            "HOME submitted, so the turn is in flight and RIVAL is what it waits on"
        );

        // A later command in the same turn must not push the deadline out.
        let deadline = gate.deadline().expect("armed");
        assert!(matches!(
            gate.assess(&[RIVAL], &seats, opened + Duration::from_millis(10), limits()),
            TurnWait::Wait { deadline: d, .. } if d == deadline
        ));

        assert_eq!(
            gate.assess(&[RIVAL], &seats, deadline, limits()),
            TurnWait::ResolveOnTimeout {
                silent_seats: vec![RIVAL]
            }
        );
        assert_eq!(gate.deadline(), None, "and the wait is over");
    }

    /// The other way the wait ends: the silent seat answers before the deadline.
    #[test]
    fn a_seat_that_answers_in_time_resolves_the_turn_without_the_timeout() {
        let mut gate = SeatTurnGate::default();
        let mut seats = SeatRegistry::default();
        seats.claim(HOME, FIRST, &ROSTER).expect("home");
        seats.claim(RIVAL, SECOND, &ROSTER).expect("rival");

        let opened = Instant::now();
        assert!(matches!(
            gate.assess(&[RIVAL], &seats, opened, limits()),
            TurnWait::Wait { .. }
        ));
        assert_eq!(
            gate.assess(&[], &seats, opened + Duration::from_millis(1), limits()),
            TurnWait::Resolve
        );
        assert_eq!(gate.deadline(), None);
    }
}
