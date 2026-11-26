# API versioning policy

MiraclePay's HTTP API is versioned in the path. The current
version is **v1**; every endpoint lives under `/api/v1/...`.

## Discovering versions

```
GET /api/versions
→ { "current": "v1", "supported": ["v1"], "deprecated": [], "policy": "…" }
```

Every `/api/v1` response carries an `API-Version: v1` header.
Requests to an unserved version (e.g. `/api/v2/...`) return
`404` with a structured body:

```json
{ "error": "unsupported_api_version", "current": "v1", "supported": ["v1"] }
```

## The rules

1. **Additive changes ship within v1.** New endpoints, new
   optional request fields, and new response fields are not
   breaking and need no new version. Clients must ignore unknown
   response fields.
2. **Breaking changes ship as a new version.** Removing or
   renaming a field, changing a field's type or meaning, removing
   an endpoint, or tightening validation in a way existing valid
   requests would fail — all of these require `/api/v2`.
3. **Overlap window.** When a new version launches, the previous
   version keeps working for at least 6 months. Its responses
   gain a `Sunset` header (RFC 8594) with the removal date once
   that date is set, and `/api/versions` lists it under
   `deprecated`.
4. **App version ≠ API version.** The product is v0.0.1; the API
   is v1. They evolve independently — the OpenAPI document's
   `info.version` tracks the product release, the path tracks the
   API contract.
5. **SDKs pin a version.** The JS SDK targets v1 and will keep
   working unchanged against any v1 deployment.
