# The help text embeds a markdown table; the tables op re-pads it inside
# the string literal (a whitespace-only change: the words stay identical).
HELP = """
| Flag | Effect |
| --- | --- |
| -a | append |
| --very-long-flag | append with everything |
"""


def run():
    print(HELP)
