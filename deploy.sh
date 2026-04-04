#!/bin/bash

# Leaf Deployment Script
# サーバー上の .env を保護しつつ、フロントエンドとバックエンドを同期します。

SERVER="157.7.198.188"
PORT="22001"
USER="kow"
REMOTE_PATH="/var/wwws/digitarhythm.net/leaf"

echo "🚀 Starting deployment to $SERVER..."

# 1. Frontend Build
echo "📦 Building frontend..."
trunk build --release --public-url /
if [ $? -ne 0 ]; then
    echo "❌ Build failed. Deployment aborted."
    exit 1
fi

# 1.5. wasm-opt 最適化
if command -v wasm-opt &> /dev/null; then
    WASM_FILE=$(ls dist/*_bg.wasm 2>/dev/null)
    if [ -n "$WASM_FILE" ]; then
        echo "🔧 Optimizing WASM with wasm-opt..."
        wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals "$WASM_FILE" -o "${WASM_FILE}.opt" && mv "${WASM_FILE}.opt" "$WASM_FILE"
        echo "   $(ls -lh $WASM_FILE | awk '{print $5}') after wasm-opt"
    fi
fi

# 2. Sync Frontend Assets (Excluding server configs and dependencies)
echo "🚚 Syncing frontend assets..."
rsync -avz --checksum --delete \
    --exclude '.env' \
    --exclude 'server/' \
    --exclude 'node_modules/' \
    -e "ssh -p $PORT" \
    dist/ $USER@$SERVER:$REMOTE_PATH
if [ $? -ne 0 ]; then
    echo "❌ Frontend sync failed."
    exit 1
fi

# 3. Sync Backend Code (Excluding dependencies and env)
echo "🚚 Syncing backend code..."
rsync -avz \
    --exclude 'node_modules/' \
    --exclude '.env' \
    --exclude '.git/' \
    -e "ssh -p $PORT" \
    server/ $USER@$SERVER:$REMOTE_PATH/server
if [ $? -ne 0 ]; then
    echo "❌ Backend sync failed."
    exit 1
fi

# 4. Restart Backend via PM2
echo "🔄 Restarting backend service..."
ssh -p $PORT $USER@$SERVER "cd $REMOTE_PATH/server && pm2 startOrRestart ecosystem.config.js --update-env"
if [ $? -ne 0 ]; then
    echo "❌ PM2 restart failed."
    exit 1
fi

# 5. Deploy nginx config and reload
NGINX_CONF="nginx/leaf.digitarhythm.net.conf"
NGINX_SITE="leaf.digitarhythm.net"
if [ -f "$NGINX_CONF" ]; then
    echo "🚚 Deploying nginx config..."
    scp -P $PORT $NGINX_CONF $USER@$SERVER:/tmp/$NGINX_SITE.conf
    ssh -p $PORT $USER@$SERVER "sudo cp /tmp/$NGINX_SITE.conf /etc/nginx/sites-available/$NGINX_SITE && sudo nginx -t && sudo systemctl reload nginx"
    if [ $? -ne 0 ]; then
        echo "❌ nginx reload failed. Check config manually."
        exit 1
    fi
    echo "✅ nginx config deployed and reloaded."
fi

echo "✅ Deployment successful!"
say "デプロイが正常に完了しました。"
