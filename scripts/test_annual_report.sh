#!/bin/bash

# Test script for annual report API
# Usage: ./scripts/test_annual_report.sh [fid] [year]

set -e

API_URL="${API_URL:-http://localhost:3000}"
FID="${1:-460432}"  # Default FID from previous investigation
YEAR="${2:-2024}"   # Default year

echo "🧪 Testing Annual Report API"
echo "============================"
echo "API URL: $API_URL"
echo "FID: $FID"
echo "Year: $YEAR"
echo ""

# Check if API server is running
echo "📡 Checking API health..."
if ! curl -s -f "$API_URL/api/health" > /dev/null; then
    echo "❌ API server is not running at $API_URL"
    echo "   Please start the server with: cargo run -- serve api"
    exit 1
fi
echo "✅ API server is running"
echo ""

# Test annual report endpoint
echo "📊 Fetching annual report for FID $FID, year $YEAR..."
RESPONSE=$(curl -s -w "\n%{http_code}" "$API_URL/api/users/$FID/annual-report/$YEAR")
HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

echo "HTTP Status: $HTTP_CODE"
echo ""

if [ "$HTTP_CODE" != "200" ]; then
    echo "❌ API request failed with status $HTTP_CODE"
    echo "Response:"
    echo "$BODY" | jq '.' 2>/dev/null || echo "$BODY"
    exit 1
fi

echo "✅ API request successful"
echo ""

# Parse and display key fields
echo "📋 Annual Report Summary:"
echo "========================="
echo "$BODY" | jq -r '
  "Year: " + (.data.year | tostring),
  "",
  "User:",
  "  FID: " + (.data.user.fid | tostring),
  "  Username: " + (.data.user.username // "null" | tostring),
  "  Display Name: " + (.data.user.display_name // "null" | tostring),
  "",
  "Activity:",
  "  Total Casts (all time): " + (.data.activity.total_casts | tostring),
  "  Total Casts in Year: " + (.data.activity.total_casts_in_year | tostring),
  "",
  "Content Style:",
  "  Top Emojis: " + (.data.content_style.top_emojis | length | tostring) + " items",
  "  Top Words: " + (.data.content_style.top_words | length | tostring) + " items",
  "  Frames Used: " + (.data.content_style.frames_used | tostring),
  "",
  "Engagement:",
  "  Reactions Received: " + (.data.engagement.reactions_received | tostring),
  "  Recasts Received: " + (.data.engagement.recasts_received | tostring),
  "  Replies Received: " + (.data.engagement.replies_received | tostring),
  "",
  "Social Growth:",
  "  Current Followers: " + (.data.social_growth.current_followers | tostring),
  "  Current Following: " + (.data.social_growth.current_following | tostring)
' 2>/dev/null || echo "Failed to parse JSON response"

echo ""
echo "📝 Top Emojis:"
echo "$BODY" | jq -r '.data.content_style.top_emojis[] | "  \(.emoji): \(.count)"' 2>/dev/null | head -10 || echo "  No emojis found"

echo ""
echo "📝 Top Words:"
echo "$BODY" | jq -r '.data.content_style.top_words[] | "  \(.word): \(.count)"' 2>/dev/null | head -10 || echo "  No words found"

echo ""
echo "✅ Test completed successfully!"
echo ""
echo "Full response saved to: /tmp/annual_report_response.json"
echo "$BODY" > /tmp/annual_report_response.json

