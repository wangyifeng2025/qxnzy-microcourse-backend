#!/bin/sh
set -e

echo "Waiting for database to be ready..."
until pg_isready -h db -p 5432 -U postgres > /dev/null 2>&1; do
  sleep 1
done
echo "Database is ready."

echo "Running database migrations..."
sqlx migrate run
echo "Migrations completed successfully."

echo "Starting application..."
exec "$@"