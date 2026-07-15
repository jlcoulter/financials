#!/bin/sh
# Litestream sidecar wrapper: watches for config changes and restarts replication.
# If the config file doesn't exist yet, polls until it appears.
# When config is removed (backups disabled), waits for it to return.

CONFIG="/config/litestream.yml"
CHECK_INTERVAL=5

echo "Litestream sidecar starting..."

while true; do
    if [ ! -f "$CONFIG" ]; then
        echo "No config found at $CONFIG — waiting for backup to be enabled..."
        while [ ! -f "$CONFIG" ]; do
            sleep $CHECK_INTERVAL
        done
        echo "Config appeared, starting replication..."
    fi

    # Record config checksum so we can detect changes
    CONFIG_HASH=$(md5sum "$CONFIG" | cut -d' ' -f1)

    echo "Starting litestream replicate..."
    litestream replicate -config "$CONFIG" &
    LITESTREAM_PID=$!

    # Monitor for config changes while litestream runs
    while kill -0 $LITESTREAM_PID 2>/dev/null; do
        if [ ! -f "$CONFIG" ]; then
            echo "Config removed, stopping litestream..."
            kill $LITESTREAM_PID 2>/dev/null
            wait $LITESTREAM_PID 2>/dev/null
            break
        fi

        NEW_HASH=$(md5sum "$CONFIG" 2>/dev/null | cut -d' ' -f1)
        if [ "$NEW_HASH" != "$CONFIG_HASH" ]; then
            echo "Config changed, restarting litestream..."
            kill $LITESTREAM_PID 2>/dev/null
            wait $LITESTREAM_PID 2>/dev/null
            break
        fi

        sleep $CHECK_INTERVAL
    done

    # If litestream exited and config still exists, wait before restarting
    if [ -f "$CONFIG" ]; then
        echo "Litestream exited, restarting in 10s..."
        sleep 10
    fi
done