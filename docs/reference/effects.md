# Effects

The queue an application puts its requests on, from `Application::effects`. Each request that has an answer names the message the answer becomes. [Effects and services](../guide/effects.md) is the guide.

## Every method

{{api crates/morningstar-ui/src/effects.rs Effects except=is_empty}}

## HttpRequest

{{api crates/morningstar-core/src/services.rs HttpRequest}}

| Method | |
| --- | --- |
| `HttpMethod::Get`, `Post`, `Put`, `Patch`, `Delete`, `Head` | The methods there are |

## HttpResponse

{{api crates/morningstar-core/src/services.rs HttpResponse}}

The status is the public field `status`.

## HttpError

{{variants crates/morningstar-core/src/services.rs HttpError}}

## Recording and AudioClip

{{api crates/morningstar-core/src/services.rs Recording}}

{{api crates/morningstar-core/src/services.rs AudioClip}}

## AudioError

{{variants crates/morningstar-core/src/services.rs AudioError}}

## LiveJoin

{{api crates/morningstar-core/src/services.rs LiveJoin}}

{{api crates/morningstar-core/src/services.rs LiveBackground}}

## LiveEvent

{{variants crates/morningstar-core/src/services.rs LiveEvent}}

## LiveDisconnect

{{variants crates/morningstar-core/src/services.rs LiveDisconnect}}

## LiveError

{{variants crates/morningstar-core/src/services.rs LiveError}}

## LiveAudioRoute

{{variants crates/morningstar-core/src/services.rs LiveAudioRoute}}
