import random
import time

levels = ["INFO", "WARN", "ERROR", "DEBUG"]
endpoints = ["/api/v1/users", "/api/v1/auth", "/healthz", "/api/v1/data"]

with open("seed_logs.txt", "w") as f:
    # Generate ~100MB of logs (approx 2 million lines)
    for _ in range(2_000_000):
        level = random.choices(levels, weights=[70, 15, 5, 10])[0]
        endpoint = random.choice(endpoints)
        ms = random.randint(10, 500)
        timestamp = time.strftime('%Y-%m-%dT%H:%M:%S.000Z', time.gmtime())
        f.write(f"[{timestamp}] {level} - GET {endpoint} {ms}ms\n")

# Then run this in the shell to generate a 10GB log file:
# $ for i in {1..100}; do cat seed_logs.txt >> 10gb_massive_log.txt; done