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

Currently, the API does not require authentication. However, rate limiting may be implemented in production environments.

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

## Versioning

The API version is included in the health check endpoint response. Future versions may introduce breaking changes, which will be documented in release notes.

## Support

For issues, questions, or contributions, please refer to the main README.md file.

