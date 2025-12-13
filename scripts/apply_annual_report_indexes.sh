#!/bin/bash
# Apply annual report indexes to the database
# Usage: ./scripts/apply_annual_report_indexes.sh

set -e

# Load database URL from config or environment
DB_URL="${DATABASE_URL:-postgresql://localhost/snaprag}"

echo "🔍 Applying annual report indexes..."
echo "Database: $DB_URL"
echo ""

# Check if psql is available
if ! command -v psql &> /dev/null; then
    echo "❌ Error: psql not found. Please install PostgreSQL client tools."
    exit 1
fi

# Apply indexes
psql "$DB_URL" -f migrations/001_annual_report_indexes.sql

echo ""
echo "✅ Annual report indexes applied successfully!"
echo ""
echo "📊 To monitor index usage, run:"
echo "   psql $DB_URL -c \"SELECT indexname, idx_scan, idx_tup_read FROM pg_stat_user_indexes WHERE indexname LIKE 'idx_%annual%' OR indexname LIKE 'idx_casts_fid_timestamp' OR indexname LIKE 'idx_links_target_fid%' ORDER BY idx_scan DESC;\""

