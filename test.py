import requests
import timeit
import sys
import concurrent.futures
from time import sleep


IP_BENCH = "127.0.0.1"
IP_TEST = "129.80.117.126"
IP = IP_BENCH


def make_requests():
    res = requests.get(
        "https://ifconfig.me/",
        proxies={
            "http": f"http://{IP}:1080",
            "https": f"http://{IP}:1080",
        },
    )

    print(res.text)


if len(sys.argv) > 1:
    if sys.argv[1] == "single":
        make_requests()
        sys.exit(0)

while True:
    try:
        make_requests()
    except Exception as e:
        print(e)
        sleep(0.3)
        continue
    break


def do_timeit():
    timeit.timeit(make_requests, number=15)


with concurrent.futures.ThreadPoolExecutor(max_workers=5) as executor:
    futures = [executor.submit(do_timeit) for _ in range(5)]
    for future in concurrent.futures.as_completed(futures):
        try:
            future.result()
        except Exception as e:
            print(e)
requests.get(
    f"http://{IP}:1080/shutdown",
)
