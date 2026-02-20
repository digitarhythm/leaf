module.exports = {
  apps : [{
    name: "leaf-backend",
    script: "./index.js",
    cwd: "/var/wwws/digitarhythm.net/leaf/server",
    instances: 1,
    autorestart: true,
    watch: false,
    max_memory_restart: '1G',
    env: {
      NODE_ENV: "production",
    },
    // 絶対パスで指定して確実に読み込ませる
    env_file: "/var/wwws/digitarhythm.net/leaf/.env"
  }]
};
