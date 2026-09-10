let primary = ConnectionOptions {
    host: "primary.internal",
    port: 443,
    tls: true,
    timeout_secs: 30,
    retries: 3,
    keep_alive: true,
};
let backup = ConnectionOptions {
    host: "backup.internal",
    port: 443,
    tls: true,
    timeout_secs: 30,
    retries: 3,
    keep_alive: true,
};
let metrics = ConnectionOptions {
    host: "metrics.internal",
    port: 443,
    tls: true,
    timeout_secs: 30,
    retries: 3,
    keep_alive: true,
};
