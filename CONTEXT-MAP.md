# Context map

Petal has one meeting vocabulary: [Petal meetings](CONTEXT.md). This map describes the integration surfaces relevant to the native Linux milestone, not newly independent bounded contexts.

## Surfaces

- **Native desktop app** — `apps/desktop/`; room controls, microphone/speaker audio, and independent remote windows.
- **Hosted room backend** — `backend/`; room access and scoped tokens. Existing clients must not need different servers for Linux interoperability.
- **LiveKit transport** — the desktop app's `src-tauri/src/transport/`; signaling, media, publication metadata, and participant events.
- **Shared presentation and logic** — `shared/`; common meeting language and reusable UI behavior. The browser app is another consumer, not the Linux milestone's implementation or acceptance peer.

## Relationships

- Native desktop app → hosted backend: request room membership and scoped access.
- Native desktop app ↔ LiveKit: publish microphone audio, receive remote audio and the supported incoming share, exchange meeting presence.
- Native sharer → Linux receiver: a share becomes a remote window; unpublishing or leaving retires that presentation.

## Canonical planning sources

- [Interview-derived milestone requirements](docs/requirements/native-linux-receive.md)
- [Milestone scope decision](docs/adrs/0001-native-linux-receive-only.md)
- [Native media seam decision](docs/adrs/0002-linux-native-media-seams.md)
- [Build plan](plans/2026-10-14-native-linux-receive.md)

Current implemented architecture and validation limits remain documented in [ARCHITECTURE.md](docs/ARCHITECTURE.md) and [VALIDATION.md](docs/VALIDATION.md). These planning documents do not claim Linux support has shipped.
