# Context map

Petal has one meeting vocabulary: [Petal meetings](CONTEXT.md). This map describes the integration surfaces relevant to the native Linux milestone, not newly independent bounded contexts.

## Surfaces

- **Native desktop app** — `apps/desktop/`; room controls, microphone/speaker audio, and independent remote windows.
- **Hosted room backend** — `backend/`; room access and scoped tokens. For acceptance, Linux and the released Windows native client explicitly use the same Tommy-owned self-hosted stateless backend and LiveKit service, not separate interoperability servers.
- **LiveKit transport** — the desktop app's `src-tauri/src/transport/`; signaling, media, publication metadata, and participant events.
- **Shared presentation and logic** — `shared/`; common meeting language and reusable UI behavior. The browser app is another consumer, not the Linux milestone's implementation or acceptance peer.

## Relationships

- Native desktop app → owned acceptance backend: request room membership and scoped access; server API secrets remain on the backend/LiveKit service, not either native client.
- Native desktop app ↔ owned LiveKit service: publish microphone audio, receive remote audio and the supported incoming share, exchange meeting presence. The real backend and LiveKit must be deployed and reachable from both clients; a mock or dev-server path is not media acceptance.
- Native sharer → Linux receiver: a share becomes a remote window; unpublishing or leaving retires that presentation.

Tommy selected self-hosted acceptance on 2026-10-27, replacing the upstream-hosted-room requirement and removing upstream operator access as a prerequisite. Earlier upstream-hosted runtime observations remain historical evidence. Separate owned-stack checks now prove trusted TLS, native Linux/Windows mutual membership and join/configuration recovery; physical audio and incoming-share acceptance remain pending.

## Canonical planning sources

- [Interview-derived milestone requirements](docs/requirements/native-linux-receive.md)
- [Milestone scope decision](docs/adrs/0001-native-linux-receive-only.md)
- [Native media seam decision](docs/adrs/0002-linux-native-media-seams.md)
- [Build plan](plans/2026-10-14-native-linux-receive.md)

Current implemented architecture and validation limits remain documented in [ARCHITECTURE.md](docs/ARCHITECTURE.md) and [VALIDATION.md](docs/VALIDATION.md). These planning documents do not claim Linux support has shipped.
