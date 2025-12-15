# SnapRAG API Documentation

## Overview

SnapRAG provides a comprehensive RESTful API for accessing Farcaster protocol data, including user profiles, casts, engagement metrics, social graph analysis, and AI-powered features like RAG (Retrieval-Augmented Generation) and chat sessions.

**Base URL**: `http://localhost:3000/api`

All API responses follow a standard format:

```json
{
  "success": true,
  "data": { ... },
  "error": null
}
```

## Authentication

SnapRAG API supports optional request signing authentication to prevent replay attacks and unauthorized access. Authentication can be enabled or disabled via configuration.

### Overview

When authentication is enabled, all API requests must include:
- **X-Token**: Token identifier (configured on the server)
- **X-Timestamp**: Unix timestamp in seconds (for replay protection)
- **X-Signature**: Base64-encoded HMAC-SHA256 signature of the request

### Token Management

#### Generate a Token

Use the CLI to generate a new token/secret pair:

```bash
snaprag auth generate --name production_client
```

This will:
1. Generate a random 32-byte secret (hex-encoded)
2. Display the token name and secret
3. Optionally update `config.toml` if it exists

**Example output:**
```
Generated new token: production_client
Secret: a1b2c3d4e5f6...

Add this to your config.toml:
[auth]
enabled = true
production_client = "a1b2c3d4e5f6..."
```

#### List Tokens

View all configured tokens:

```bash
snaprag auth list
```

#### Revoke a Token

Remove a token from the configuration:

```bash
snaprag auth revoke production_client
```

### Request Signing

#### Signature Format

The signature is computed over the following string (newline-separated):

```
{method}
{path}
{query_string}
{body_hash}
{timestamp}
```

Where:
- `method`: HTTP method (GET, POST, etc.)
- `path`: Request path (e.g., `/api/profiles/123`)
- `query_string`: URL query string (empty if none)
- `body_hash`: SHA256 hash of request body, hex-encoded (empty string for GET/HEAD/DELETE)
- `timestamp`: Unix timestamp in seconds (same as X-Timestamp header)

#### Signature Algorithm

1. Build the signature string as described above
2. Compute HMAC-SHA256 using the secret key (hex-decoded)
3. Base64-encode the HMAC result
4. Set as `X-Signature` header

#### Timestamp Validation

The server validates that the timestamp is within ±5 minutes of the current time to prevent replay attacks. Requests with timestamps outside this window will be rejected.

### Configuration

Add authentication configuration to `config.toml`:

```toml
[auth]
enabled = true
production_client = "hex_encoded_secret_key_here"
test_client = "another_secret_key_here"
```

Multiple tokens can be configured for different clients or environments.

### Examples

#### Bash/Shell

```bash
#!/bin/bash

TOKEN="production_client"
SECRET="a1b2c3d4e5f6..."  # Hex-encoded secret
METHOD="GET"
PATH="/api/profiles/123"
QUERY=""
BODY_HASH=""
TIMESTAMP=$(date +%s)

# Build signature string
SIG_STRING="${METHOD}\n${PATH}\n${QUERY}\n${BODY_HASH}\n${TIMESTAMP}"

# Compute HMAC-SHA256 and base64 encode
SIGNATURE=$(echo -n -e "$SIG_STRING" | openssl dgst -sha256 -hmac "$(echo -n "$SECRET" | xxd -r -p)" -binary | base64)

# Make request
curl -X "$METHOD" "http://localhost:3000${PATH}" \
  -H "X-Token: $TOKEN" \
  -H "X-Timestamp: $TIMESTAMP" \
  -H "X-Signature: $SIGNATURE"
```

#### Python

```python
import hmac
import hashlib
import base64
import time
import requests

def sign_request(method, path, query, body, secret_hex, timestamp):
    """Sign a request for SnapRAG API"""
    # Build signature string
    body_hash = hashlib.sha256(body.encode() if body else b'').hexdigest() if body else ''
    sig_string = f"{method}\n{path}\n{query}\n{body_hash}\n{timestamp}"
    
    # Decode secret from hex
    secret_bytes = bytes.fromhex(secret_hex)
    
    # Compute HMAC-SHA256
    hmac_digest = hmac.new(secret_bytes, sig_string.encode(), hashlib.sha256).digest()
    
    # Base64 encode
    signature = base64.b64encode(hmac_digest).decode()
    
    return signature

# Example usage
TOKEN = "production_client"
SECRET = "a1b2c3d4e5f6..."  # Hex-encoded
METHOD = "GET"
PATH = "/api/profiles/123"
QUERY = ""
BODY = ""
TIMESTAMP = int(time.time())

signature = sign_request(METHOD, PATH, QUERY, BODY, SECRET, TIMESTAMP)

response = requests.get(
    f"http://localhost:3000{PATH}",
    headers={
        "X-Token": TOKEN,
        "X-Timestamp": str(TIMESTAMP),
        "X-Signature": signature
    }
)
print(response.json())
```

#### JavaScript/Node.js

```javascript
const crypto = require('crypto');

function signRequest(method, path, query, body, secretHex, timestamp) {
    // Build signature string
    const bodyHash = body 
        ? crypto.createHash('sha256').update(body).digest('hex')
        : '';
    const sigString = `${method}\n${path}\n${query}\n${bodyHash}\n${timestamp}`;
    
    // Decode secret from hex
    const secretBytes = Buffer.from(secretHex, 'hex');
    
    // Compute HMAC-SHA256
    const hmac = crypto.createHmac('sha256', secretBytes);
    hmac.update(sigString);
    const signature = hmac.digest('base64');
    
    return signature;
}

// Example usage
const TOKEN = 'production_client';
const SECRET = 'a1b2c3d4e5f6...';  // Hex-encoded
const METHOD = 'GET';
const PATH = '/api/profiles/123';
const QUERY = '';
const BODY = '';
const TIMESTAMP = Math.floor(Date.now() / 1000);

const signature = signRequest(METHOD, PATH, QUERY, BODY, SECRET, TIMESTAMP);

fetch(`http://localhost:3000${PATH}`, {
    method: METHOD,
    headers: {
        'X-Token': TOKEN,
        'X-Timestamp': TIMESTAMP.toString(),
        'X-Signature': signature
    }
})
.then(res => res.json())
.then(data => console.log(data));
```

#### POST Request Example

For POST requests with a body:

```python
import json

METHOD = "POST"
PATH = "/api/search/profiles"
QUERY = ""
BODY = json.dumps({"query": "developer", "limit": 10})
TIMESTAMP = int(time.time())

# Hash the body
body_hash = hashlib.sha256(BODY.encode()).hexdigest()

# Build signature string
sig_string = f"{METHOD}\n{PATH}\n{QUERY}\n{body_hash}\n{TIMESTAMP}"

# Sign and send
signature = sign_request(METHOD, PATH, QUERY, BODY, SECRET, TIMESTAMP)

response = requests.post(
    f"http://localhost:3000{PATH}",
    headers={
        "X-Token": TOKEN,
        "X-Timestamp": str(TIMESTAMP),
        "X-Signature": signature,
        "Content-Type": "application/json"
    },
    data=BODY
)
```

### Error Responses

When authentication fails, the API returns `401 Unauthorized`:

```json
{
  "success": false,
  "data": null,
  "error": "Authentication failed: Missing X-Token header"
}
```

Common error scenarios:
- Missing required headers (X-Token, X-Timestamp, X-Signature)
- Invalid token (token not found in configuration)
- Invalid signature (signature mismatch)
- Timestamp out of window (replay attack protection)
- Invalid timestamp format

### Security Notes

1. **Keep secrets secure**: Never commit secrets to version control
2. **Use HTTPS in production**: Always use HTTPS to protect secrets in transit
3. **Rotate tokens regularly**: Use `snaprag auth revoke` to remove compromised tokens
4. **Time synchronization**: Ensure client clocks are synchronized (NTP recommended)
5. **Constant-time comparison**: The server uses constant-time comparison to prevent timing attacks

## Endpoints

### Health & Status

#### GET `/api/health`

Check API health status.

**Response:**
```json
{
  "success": true,
  "data": {
    "status": "ok",
    "version": "0.1.0"
  }
}
```

#### GET `/api/stats`

Get overall statistics about the database.

**Response:**
```json
{
  "success": true,
  "data": {
    "total_profiles": 1393099,
    "total_casts": 224749776,
    "profiles_with_embeddings": 1000000,
    "casts_with_embeddings": 50000000,
    "cache_stats": {
      "hits": 1000,
      "misses": 100,
      "hit_rate": 0.909,
      "evictions": 10,
      "expired_cleanups": 5,
      "profile_entries": 500,
      "social_entries": 200,
      "mbti_entries": 100,
      "total_entries": 800,
      "max_entries": 1000,
      "usage_percentage": 80.0
    }
  }
}
```

### User Profiles

#### GET `/api/profiles`

List all user profiles with pagination.

**Query Parameters:**
- `limit` (optional): Maximum number of results (default: 20)
- `offset` (optional): Number of results to skip (default: 0)

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "fid": 1,
      "username": "alice",
      "display_name": "Alice",
      "bio": "Developer",
      "pfp_url": "https://...",
      "location": "San Francisco",
      "twitter_username": "alice",
      "github_username": "alice",
      "registered_at": 1609459200,
      "total_casts": 1000,
      "total_reactions": 500,
      "total_links": 200
    }
  ]
}
```

#### GET `/api/profiles/:fid`

Get profile by Farcaster ID (FID).

**Path Parameters:**
- `fid`: Farcaster ID (integer)

**Response:**
```json
{
  "success": true,
  "data": {
    "fid": 1,
    "username": "alice",
    "display_name": "Alice",
    "bio": "Developer",
    "pfp_url": "https://...",
    "location": "San Francisco",
    "twitter_username": "alice",
    "github_username": "alice",
    "registered_at": 1609459200,
    "total_casts": 1000,
    "total_reactions": 500,
    "total_links": 200
  }
}
```

#### GET `/api/profiles/username/:username`

Get profile by username.

**Path Parameters:**
- `username`: Farcaster username (without @)

**Response:** Same as GET `/api/profiles/:fid`

#### GET `/api/profiles/address/:address`

Get profile by address (supports multiple chains).

**Path Parameters:**
- `address`: Wallet address (hex-encoded)

**Response:** Same as GET `/api/profiles/:fid`

#### GET `/api/profiles/address/ethereum/:address`

Get profile by Ethereum address.

**Path Parameters:**
- `address`: Ethereum address (hex-encoded)

**Response:** Same as GET `/api/profiles/:fid`

#### GET `/api/profiles/address/solana/:address`

Get profile by Solana address.

**Path Parameters:**
- `address`: Solana address (base58-encoded)

**Response:** Same as GET `/api/profiles/:fid`

### User Engagement Metrics

#### GET `/api/users/:fid/engagement`

Get engagement metrics for a user (reactions, recasts, replies received).

**Path Parameters:**
- `fid`: Farcaster ID (integer)

**Query Parameters:**
- `start_timestamp` (optional): Start time (Unix timestamp in seconds)
- `end_timestamp` (optional): End time (Unix timestamp in seconds)

**Response:**
```json
{
  "success": true,
  "data": {
    "reactions_received": 1500,
    "recasts_received": 300,
    "replies_received": 200,
    "total_engagement": 2000,
    "most_popular_cast": {
      "message_hash": "0x...",
      "text": "Hello world!",
      "reactions": 100,
      "recasts": 50,
      "replies": 25,
      "timestamp": 1609459200
    },
    "top_reactors": [
      {
        "fid": 2,
        "username": "bob",
        "display_name": "Bob",
        "interaction_count": 50
      }
    ]
  }
}
```

### Temporal Activity

#### GET `/api/users/:fid/activity/temporal`

Get temporal activity analysis (hourly and monthly distribution).

**Path Parameters:**
- `fid`: Farcaster ID (integer)

**Query Parameters:**
- `start_timestamp` (optional): Start time (Unix timestamp in seconds)
- `end_timestamp` (optional): End time (Unix timestamp in seconds)

**Response:**
```json
{
  "success": true,
  "data": {
    "hourly_distribution": [
      {"hour": 0, "count": 10},
      {"hour": 1, "count": 5},
      ...
    ],
    "monthly_distribution": [
      {"month": "2024-01", "count": 100},
      {"month": "2024-02", "count": 150},
      ...
    ],
    "most_active_hour": 14,
    "most_active_month": "2024-06",
    "first_cast": {
      "message_hash": "0x...",
      "text": "First cast",
      "timestamp": 1609459200
    },
    "last_cast": {
      "message_hash": "0x...",
      "text": "Latest cast",
      "timestamp": 1704067200
    }
  }
}
```

### Content Style Analysis

#### GET `/api/users/:fid/content/style`

Get content style analysis (emojis, text length, frames).

**Path Parameters:**
- `fid`: Farcaster ID (integer)

**Query Parameters:**
- `start_timestamp` (optional): Start time (Unix timestamp in seconds)
- `end_timestamp` (optional): End time (Unix timestamp in seconds)

**Response:**
```json
{
  "success": true,
  "data": {
    "top_emojis": [
      {"emoji": "🔥", "count": 50},
      {"emoji": "💎", "count": 30},
      {"emoji": "🚀", "count": 20}
    ],
    "avg_cast_length": 120.5,
    "total_characters": 120500,
    "frames_used": 10,
    "frames_created": 0,
    "channels_created": 0
  }
}
```

### Follower Growth

#### GET `/api/users/:fid/followers/growth`

Get follower growth metrics and historical snapshots.

**Path Parameters:**
- `fid`: Farcaster ID (integer)

**Query Parameters:**
- `start_timestamp` (optional): Start time (Unix timestamp in seconds)
- `end_timestamp` (optional): End time (Unix timestamp in seconds)

**Response:**
```json
{
  "success": true,
  "data": {
    "current_followers": 1000,
    "current_following": 500,
    "followers_at_start": 800,
    "following_at_start": 400,
    "net_growth": 200,
    "monthly_snapshots": [
      {"month": "2024-01", "followers": 800, "following": 400},
      {"month": "2024-02", "followers": 850, "following": 420},
      ...
    ]
  }
}
```

### Domain & Username Status

#### GET `/api/users/:fid/domains`

Get domain and username status (ENS, Farcaster name).

**Path Parameters:**
- `fid`: Farcaster ID (integer)

**Response:**
```json
{
  "success": true,
  "data": {
    "has_ens": true,
    "ens_name": "alice.eth",
    "has_farcaster_name": true,
    "farcaster_name": "alice",
    "username_type": "fname"
  }
}
```

### Annual Report

#### GET `/api/users/:fid/annual-report/:year`

Get comprehensive annual report combining all metrics.

**Path Parameters:**
- `fid`: Farcaster ID (integer)
- `year`: Year (e.g., 2024)

**Response:**
```json
{
  "success": true,
  "data": {
    "fid": 1,
    "username": "alice",
    "display_name": "Alice",
    "year": 2024,
    "engagement": { ... },
    "temporal_activity": { ... },
    "content_style": { ... },
    "follower_growth": { ... },
    "domain_status": { ... },
    "network_comparison": { ... }
  }
}
```

### Network Statistics

#### GET `/api/stats/network/averages`

Get network-wide statistics and percentiles for comparison.

**Query Parameters:**
- `start_timestamp` (optional): Start time (Unix timestamp in seconds)
- `end_timestamp` (optional): End time (Unix timestamp in seconds)

**Response:**
```json
{
  "success": true,
  "data": {
    "avg_casts_per_user": 150.5,
    "avg_reactions_per_user": 75.2,
    "avg_followers_per_user": 200.3,
    "total_active_users": 1000000,
    "percentiles": {
      "casts": {
        "p50": 100,
        "p75": 200,
        "p90": 500
      },
      "reactions": {
        "p50": 50,
        "p75": 100,
        "p90": 250
      }
    }
  }
}
```

### Cast Statistics

#### GET `/api/casts/stats/:fid`

Get detailed cast statistics for a user.

**Path Parameters:**
- `fid`: Farcaster ID (integer)

**Query Parameters:**
- `start_timestamp` (optional): Start time (Unix timestamp in seconds)
- `end_timestamp` (optional): End time (Unix timestamp in seconds)

**Response:**
```json
{
  "success": true,
  "data": {
    "total_casts": 1000,
    "date_range": {
      "start": "2024-01-01T00:00:00Z",
      "end": "2024-12-31T23:59:59Z"
    },
    "date_distribution": [
      {"date": "2024-01-01", "count": 10},
      {"date": "2024-01-02", "count": 15},
      ...
    ],
    "language_distribution": {
      "en": 800,
      "zh": 100,
      "ja": 50
    },
    "top_nouns": [
      {"word": "crypto", "count": 50, "language": "en"},
      {"word": "blockchain", "count": 30, "language": "en"}
    ],
    "top_verbs": [
      {"word": "build", "count": 40, "language": "en"},
      {"word": "create", "count": 30, "language": "en"}
    ]
  }
}
```

### Social Graph Analysis

#### GET `/api/social/:fid`

Get social graph analysis for a user.

**Path Parameters:**
- `fid`: Farcaster ID (integer)

**Response:**
```json
{
  "success": true,
  "data": {
    "fid": 1,
    "following_count": 500,
    "followers_count": 1000,
    "influence_score": 2.5,
    "top_followed_users": [
      {
        "fid": 2,
        "username": "bob",
        "display_name": "Bob"
      }
    ],
    "top_followers": [
      {
        "fid": 3,
        "username": "charlie",
        "display_name": "Charlie"
      }
    ]
  }
}
```

#### GET `/api/social/username/:username`

Get social graph analysis by username.

**Path Parameters:**
- `username`: Farcaster username (without @)

**Response:** Same as GET `/api/social/:fid`

### MBTI Personality Analysis

#### GET `/api/mbti/:fid`

Get MBTI personality analysis for a user.

**Path Parameters:**
- `fid`: Farcaster ID (integer)

**Response:**
```json
{
  "success": true,
  "data": {
    "fid": 1,
    "mbti_type": "INTJ",
    "confidence": 0.85,
    "dimensions": {
      "introversion": 0.8,
      "intuition": 0.7,
      "thinking": 0.75,
      "judging": 0.8
    },
    "description": "..."
  }
}
```

#### GET `/api/mbti/username/:username`

Get MBTI analysis by username.

**Path Parameters:**
- `username`: Farcaster username (without @)

**Response:** Same as GET `/api/mbti/:fid`

#### POST `/api/mbti/batch`

Batch MBTI analysis for multiple users.

**Request Body:**
```json
{
  "fids": [1, 2, 3]
}
```

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "fid": 1,
      "mbti_type": "INTJ",
      "confidence": 0.85
    },
    ...
  ]
}
```

#### GET `/api/mbti/stats`

Get MBTI type distribution statistics.

**Response:**
```json
{
  "success": true,
  "data": {
    "total_analyzed": 10000,
    "type_distribution": {
      "INTJ": 500,
      "ENFP": 450,
      ...
    }
  }
}
```

#### GET `/api/mbti/search/:mbti_type`

Search users by MBTI type.

**Path Parameters:**
- `mbti_type`: MBTI type (e.g., "INTJ")

**Query Parameters:**
- `limit` (optional): Maximum number of results (default: 20)

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "fid": 1,
      "username": "alice",
      "display_name": "Alice",
      "mbti_type": "INTJ",
      "confidence": 0.85
    }
  ]
}
```

#### GET `/api/mbti/compatibility/:fid1/:fid2`

Get MBTI compatibility between two users.

**Path Parameters:**
- `fid1`: First user's Farcaster ID
- `fid2`: Second user's Farcaster ID

**Response:**
```json
{
  "success": true,
  "data": {
    "user1": {
      "fid": 1,
      "mbti_type": "INTJ"
    },
    "user2": {
      "fid": 2,
      "mbti_type": "ENFP"
    },
    "compatibility_score": 0.75,
    "description": "..."
  }
}
```

### Search

#### POST `/api/search/profiles`

Search profiles using semantic search.

**Request Body:**
```json
{
  "query": "developer",
  "limit": 20,
  "method": "semantic"
}
```

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "fid": 1,
      "username": "alice",
      "display_name": "Alice",
      "bio": "Developer",
      "similarity": 0.95
    }
  ]
}
```

#### POST `/api/search/casts`

Search casts using semantic search.

**Request Body:**
```json
{
  "query": "blockchain",
  "limit": 20,
  "threshold": 0.5
}
```

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "message_hash": "0x...",
      "fid": 1,
      "text": "Blockchain is the future",
      "timestamp": 1609459200,
      "similarity": 0.92
    }
  ]
}
```

### RAG (Retrieval-Augmented Generation)

#### POST `/api/rag/query`

Query the RAG system with a question.

**Request Body:**
```json
{
  "question": "What is Farcaster?",
  "retrieval_limit": 10,
  "method": "semantic",
  "temperature": 0.7,
  "max_tokens": 2000
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "answer": "Farcaster is a decentralized social protocol...",
    "sources": [
      {
        "message_hash": "0x...",
        "text": "...",
        "similarity": 0.95
      }
    ],
    "tokens_used": 150
  }
}
```

### Chat Sessions

#### POST `/api/chat/create`

Create a new chat session with a user.

**Request Body:**
```json
{
  "user": "alice",
  "context_limit": 20,
  "temperature": 0.7
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "session_id": "uuid-here",
    "fid": 1,
    "username": "alice",
    "display_name": "Alice",
    "bio": "Developer",
    "total_casts": 1000
  }
}
```

#### POST `/api/chat/message`

Send a message in a chat session.

**Request Body:**
```json
{
  "session_id": "uuid-here",
  "message": "Hello!"
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "session_id": "uuid-here",
    "message": "Hello! How can I help you?",
    "relevant_casts_count": 5,
    "conversation_length": 2
  }
}
```

#### GET `/api/chat/session`

Get chat session information.

**Query Parameters:**
- `session_id`: Session ID

**Response:**
```json
{
  "success": true,
  "data": {
    "session_id": "uuid-here",
    "fid": 1,
    "username": "alice",
    "display_name": "Alice",
    "conversation_history": [
      {
        "role": "user",
        "content": "Hello!"
      },
      {
        "role": "assistant",
        "content": "Hello! How can I help you?"
      }
    ],
    "created_at": 1609459200,
    "last_activity": 1609459300
  }
}
```

#### DELETE `/api/chat/session/:session_id`

Delete a chat session.

**Path Parameters:**
- `session_id`: Session ID

**Response:**
```json
{
  "success": true,
  "data": null
}
```

### Data Fetching

#### POST `/api/fetch/user/:fid`

Fetch user data from Snapchain (lazy loading).

**Path Parameters:**
- `fid`: Farcaster ID (integer)

**Request Body:**
```json
{
  "with_casts": true,
  "generate_embeddings": true,
  "embedding_endpoint": "http://localhost:8000/embed",
  "max_casts": 1000
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "profile": { ... },
    "casts_count": 1000,
    "embeddings_generated": 1000,
    "source": "snapchain"
  }
}
```

#### POST `/api/fetch/users`

Batch fetch multiple users.

**Request Body:**
```json
{
  "fids": [1, 2, 3],
  "with_casts": true,
  "generate_embeddings": true,
  "embedding_endpoint": "http://localhost:8000/embed"
}
```

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "profile": { ... },
      "casts_count": 1000,
      "embeddings_generated": 1000,
      "source": "snapchain"
    },
    ...
  ]
}
```

### Metrics

#### GET `/api/metrics`

Get Prometheus metrics (for monitoring).

**Response:** Prometheus format metrics

## Error Handling

All endpoints return errors in the following format:

```json
{
  "success": false,
  "data": null,
  "error": "Error message here"
}
```

**HTTP Status Codes:**
- `200 OK`: Request successful
- `400 Bad Request`: Invalid request parameters
- `401 Unauthorized`: Authentication failed (missing/invalid token, signature mismatch, timestamp out of window)
- `404 Not Found`: Resource not found
- `500 Internal Server Error`: Server error

## Rate Limiting

Rate limiting may be implemented in production. Check response headers for rate limit information:
- `X-RateLimit-Limit`: Maximum number of requests
- `X-RateLimit-Remaining`: Remaining requests
- `X-RateLimit-Reset`: Time when rate limit resets

## Timestamps

All timestamps in the API are Unix timestamps in seconds (not milliseconds). For time range queries, both `start_timestamp` and `end_timestamp` are optional. If not provided, the query will use the full available time range.

## Pagination

Endpoints that return lists support pagination via `limit` and `offset` query parameters. The default limit is 20 items.

## Examples

### Get user engagement for 2024

```bash
curl "http://localhost:3000/api/users/1/engagement?start_timestamp=1704067200&end_timestamp=1735689600"
```

### Get annual report for 2024

```bash
curl "http://localhost:3000/api/users/1/annual-report/2024"
```

### Search for profiles

```bash
curl -X POST "http://localhost:3000/api/search/profiles" \
  -H "Content-Type: application/json" \
  -d '{"query": "developer", "limit": 10}'
```

### Create a chat session

```bash
curl -X POST "http://localhost:3000/api/chat/create" \
  -H "Content-Type: application/json" \
  -d '{"user": "alice", "context_limit": 20}'
```

### Authenticated Request Example

If authentication is enabled, include the required headers:

```bash
# Set your credentials
TOKEN="production_client"
SECRET="a1b2c3d4e5f6..."  # Your hex-encoded secret
TIMESTAMP=$(date +%s)

# Build signature (simplified - see Authentication section for full implementation)
SIG_STRING="GET\n/api/profiles/123\n\n\n${TIMESTAMP}"
SIGNATURE=$(echo -n -e "$SIG_STRING" | openssl dgst -sha256 -hmac "$(echo -n "$SECRET" | xxd -r -p)" -binary | base64)

# Make authenticated request
curl -X GET "http://localhost:3000/api/profiles/123" \
  -H "X-Token: $TOKEN" \
  -H "X-Timestamp: $TIMESTAMP" \
  -H "X-Signature: $SIGNATURE"
```

## Versioning

The API version is included in the health check endpoint response. Future versions may introduce breaking changes, which will be documented in release notes.

## Support

For issues, questions, or contributions, please refer to the main README.md file.

