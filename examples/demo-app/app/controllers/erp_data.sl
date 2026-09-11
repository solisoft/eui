# Meridian Industrial — the fake company the dashboard runs.
#
# A parts distributor: three warehouses, fourteen customers, a back order it
# is not proud of. None of it is real and none of it is stored; the point is
# that a screenshot of this should be indistinguishable from a screenshot of
# an application somebody works in.
#
# Almost everything here is *generated from an index* rather than written
# out, for the reason the feed's posts are: ten thousand SKUs as a literal
# would be a megabyte of controller, and the ledger only ever lays out the
# rows in view. The arithmetic is deliberately ugly — `(i * 137) % 8800` —
# because a well-behaved sequence looks fake at a glance, and a pseudo-random
# one that is still a pure function of `i` gives the same figures on every
# round trip without a seed in the state.

ERP_DIGITS = "0123456789"

ERP_WAREHOUSES = ["Lyon", "Rotterdam", "Katowice"]

ERP_STATUSES = ["Draft", "Confirmed", "Picked", "Invoiced", "Late"]

ERP_CHANNELS = ["Web", "Phone", "EDI", "Rep"]

ERP_PART_NAMES = [
  "Hex bolt M12",
  "Flange DN80",
  "Roller bearing",
  "Gasket set",
  "Quick coupler",
  "Mounting bracket",
  "Seal ring 40",
  "Drive shaft",
  "Idler pulley",
  "Spacer sleeve",
  "Thrust washer",
  "Hydraulic hose"
]

# ---- Small change ----------------------------------------------------------

# "13 750 €" — grouped by threes with a space, which is what the invoices in
# this company's cupboard say. There is no money type in the catalogue and
# there should not be one: a widget that formatted currency would be
# formatting somebody else's.
def erp_money(n)
  erp_grouped(int(n)) + " €"
end

def erp_grouped(n)
  return "-" + erp_grouped(0 - n) if n < 0
  return str(n) if n < 1000

  erp_grouped(int(n / 1000)) + " " + erp_pad(n % 1000, 3)
end

def erp_pad(n, width)
  padded = str(n)
  while padded.chars().length() < width
    padded = "0" + padded
  end
  padded
end

# The digits in a reference, as a number — the seed a row's detail is built
# from, so "SO-24007" always tells the same story.
def erp_digits(ref)
  glyphs = (ref ?? "").chars()
  total = 0
  i = 0
  while i < glyphs.length()
    total = total * 10 + int(glyphs[i]) if ERP_DIGITS.includes?(glyphs[i])
    i = i + 1
  end
  total
end

# A day, `offset` days after the quarter this demo is set in opens. The
# calendar engine speaks ISO and so does everything that compares days.
def erp_day(offset)
  DateTime.parse("2026-09-01").add_days(offset).format("%Y-%m-%d")
end

# The customers are written out: there are fourteen of them, they carry a
# hierarchy the tree view reads, and a generated company name reads like a
# generated company name.
ERP_CUSTOMERS = [
  {
    "id": "C-101",
    "name": "Ada SARL",
    "city": "Lyon",
    "country": "FR",
    "tier": "Gold",
    "owner": "Camille Roy",
    "since": "2019-04-02",
    "open": 13750,
    "terms": "Net 30",
    "initial": "A",
    "tone": "accent.base"
  },
  {
    "id": "C-102",
    "name": "Grace Ltd",
    "city": "Manchester",
    "country": "GB",
    "tier": "Gold",
    "owner": "Tom Baril",
    "since": "2018-11-19",
    "open": 41200,
    "terms": "Net 60",
    "initial": "G",
    "tone": "info.base"
  },
  {
    "id": "C-103",
    "name": "Linus GmbH",
    "city": "Bremen",
    "country": "DE",
    "tier": "Silver",
    "owner": "Camille Roy",
    "since": "2020-02-27",
    "open": 8500,
    "terms": "Net 30",
    "initial": "L",
    "tone": "success.base"
  },
  {
    "id": "C-104",
    "name": "Margaret SA",
    "city": "Geneva",
    "country": "CH",
    "tier": "Gold",
    "owner": "Iris Vandal",
    "since": "2017-06-08",
    "open": 62400,
    "terms": "Net 45",
    "initial": "M",
    "tone": "warning.base"
  },
  {
    "id": "C-105",
    "name": "Dennis BV",
    "city": "Rotterdam",
    "country": "NL",
    "tier": "Silver",
    "owner": "Tom Baril",
    "since": "2021-09-14",
    "open": 9200,
    "terms": "Net 30",
    "initial": "D",
    "tone": "danger.base"
  },
  {
    "id": "C-106",
    "name": "Barbara LLC",
    "city": "Dublin",
    "country": "IE",
    "tier": "Bronze",
    "owner": "Iris Vandal",
    "since": "2022-01-30",
    "open": 2560,
    "terms": "On receipt",
    "initial": "B",
    "tone": "accent.base"
  },
  {
    "id": "C-107",
    "name": "Ken SAS",
    "city": "Nantes",
    "country": "FR",
    "tier": "Silver",
    "owner": "Camille Roy",
    "since": "2020-07-21",
    "open": 6400,
    "terms": "Net 30",
    "initial": "K",
    "tone": "info.base"
  },
  {
    "id": "C-108",
    "name": "Radia Inc",
    "city": "Boston",
    "country": "US",
    "tier": "Gold",
    "owner": "Iris Vandal",
    "since": "2016-03-11",
    "open": 31800,
    "terms": "Net 60",
    "initial": "R",
    "tone": "success.base"
  },
  {
    "id": "C-109",
    "name": "Katalin Kft",
    "city": "Budapest",
    "country": "HU",
    "tier": "Bronze",
    "owner": "Tom Baril",
    "since": "2023-05-04",
    "open": 1180,
    "terms": "On receipt",
    "initial": "K",
    "tone": "warning.base"
  },
  {
    "id": "C-110",
    "name": "Olsen A/S",
    "city": "Aarhus",
    "country": "DK",
    "tier": "Silver",
    "owner": "Camille Roy",
    "since": "2019-12-02",
    "open": 15300,
    "terms": "Net 30",
    "initial": "O",
    "tone": "danger.base"
  },
  {
    "id": "C-111",
    "name": "Perez y Cia",
    "city": "Bilbao",
    "country": "ES",
    "tier": "Bronze",
    "owner": "Iris Vandal",
    "since": "2022-08-17",
    "open": 3940,
    "terms": "Net 30",
    "initial": "P",
    "tone": "accent.base"
  },
  {
    "id": "C-112",
    "name": "Nowak Sp. z o.o.",
    "city": "Katowice",
    "country": "PL",
    "tier": "Silver",
    "owner": "Tom Baril",
    "since": "2021-02-09",
    "open": 11450,
    "terms": "Net 45",
    "initial": "N",
    "tone": "info.base"
  },
  {
    "id": "C-113",
    "name": "Ferrero SpA",
    "city": "Turin",
    "country": "IT",
    "tier": "Gold",
    "owner": "Camille Roy",
    "since": "2018-05-23",
    "open": 27600,
    "terms": "Net 60",
    "initial": "F",
    "tone": "success.base"
  },
  {
    "id": "C-114",
    "name": "Svensson AB",
    "city": "Gothenburg",
    "country": "SE",
    "tier": "Bronze",
    "owner": "Iris Vandal",
    "since": "2023-10-06",
    "open": 760,
    "terms": "On receipt",
    "initial": "S",
    "tone": "warning.base"
  }
]

# Where a customer sits in the list. A `for` here would make this Void to the
# type checker — and so every bit of arithmetic downstream — where a `while`
# keeps the answer an Int.
def erp_customer_index(id)
  found = 0
  i = 0
  while i < ERP_CUSTOMERS.length()
    found = i if ERP_CUSTOMERS[i]["id"] == id
    i = i + 1
  end
  found
end

def erp_customers
  ERP_CUSTOMERS
end

def erp_customer(id)
  for one in ERP_CUSTOMERS
    return one if one["id"] == id
  end

  ERP_CUSTOMERS[0]
end

# The sites under a customer, for the tree: a group, its subsidiaries, and
# the plants under those. Generated from the customer's own index so the
# shape differs between them without fourteen more literals.
def erp_sites(customer)
  i = erp_customer_index(customer["id"])
  plants = range(0, 2 + i % 2).map(fn(p) {
    {
      "id": customer["id"] + "-p" + str(p),
      "label": ERP_WAREHOUSES[(i + p) % 3] + " plant " + str(p + 1),
      "children": []
    }
  })
  [{
    "id": customer["id"],
    "label": customer["name"],
    "children": [
      {
        "id": customer["id"] + "-ops",
        "label": "Operations",
        "children": plants
      },
      {
        "id": customer["id"] + "-fin",
        "label": "Finance",
        "children": []
      }
    ]
  }]
end

# ---- Orders ----------------------------------------------------------------

# Nine pages of seven. `id` is what the grid keys rows by; every other key is
# a column id, which is the shape `data_grid` wants.
def erp_orders(count)
  range(0, count).map(fn(i) { erp_order(i) })
end

def erp_order(i)
  customer = ERP_CUSTOMERS[(i * 7) % 14]
  amount = (i * 137) % 8800 + 120
  {
    "id": "SO-24" + erp_pad(i + 1, 3),
    "ref": "SO-24" + erp_pad(i + 1, 3),
    "customer": customer["name"],
    "customer_id": customer["id"],
    "status": ERP_STATUSES[i % 5],
    "channel": ERP_CHANNELS[(i * 3) % 4],
    "due": erp_day(i * 3 % 60),
    "amount": erp_money(amount),
    "total": amount,
    "lines": 2 + i % 4
  }
end

# The lines of one order: what the table under a selected row shows. Derived
# from the order's own number, so picking a row twice gives the same lines.
def erp_lines(ref)
  seed = erp_digits(ref)
  range(0, 2 + seed % 4).map(fn(l) {
    part = ERP_PART_NAMES[(seed + l * 5) % 12]
    qty = 1 + (seed + l * 13) % 40
    unit = 4 + (seed * 3 + l * 7) % 96
    {
      "id": ref + "-" + str(l),
      "sku": "AX-" + erp_pad((seed + l * 17) % 9999, 4),
      "part": part,
      "qty": str(qty),
      "unit": erp_money(unit),
      "total": erp_money(qty * unit)
    }
  })
end

# ---- Inventory -------------------------------------------------------------

# Ten thousand of them. Never in state, never all laid out: the ledger is a
# `list`, which lays out the window in view and nothing else.
def erp_products(count)
  range(0, count).map(fn(i) { erp_product(i) })
end

def erp_product(i)
  on_hand = (i * 53) % 480
  reorder = 40 + (i * 11) % 160
  {
    "id": "AX-" + erp_pad(i + 1, 4),
    "sku": "AX-" + erp_pad(i + 1, 4),
    "name": ERP_PART_NAMES[i % 12],
    "warehouse": ERP_WAREHOUSES[i % 3],
    "on_hand": on_hand,
    "reorder": reorder,
    "short": on_hand < reorder,
    "price": erp_money(4 + (i * 7) % 240)
  }
end

# How many of the first `count` are under their reorder point — the figure
# the dashboard's stock-alert tile shows, counted rather than guessed.
def erp_short_count(count)
  erp_products(count).filter(fn(p) { p["short"] }).length()
end

# ---- The day's activity ----------------------------------------------------

def erp_activity
  [
    {
      "id": "a1",
      "who": "Camille Roy",
      "initial": "C",
      "tone": "accent.base",
      "what": "confirmed SO-24007 for Ada SARL",
      "when": "09:41"
    },
    {
      "id": "a2",
      "who": "Warehouse Lyon",
      "initial": "W",
      "tone": "info.base",
      "what": "picked 34 lines against SO-24003",
      "when": "09:12"
    },
    {
      "id": "a3",
      "who": "Tom Baril",
      "initial": "T",
      "tone": "success.base",
      "what": "raised a quote for Nowak Sp. z o.o.",
      "when": "08:57"
    },
    {
      "id": "a4",
      "who": "Billing",
      "initial": "B",
      "tone": "warning.base",
      "what": "invoiced SO-23988 — 4 210 €",
      "when": "08:30"
    },
    {
      "id": "a5",
      "who": "Iris Vandal",
      "initial": "I",
      "tone": "danger.base",
      "what": "flagged Margaret SA over its credit limit",
      "when": "08:04"
    },
    {
      "id": "a6",
      "who": "Purchasing",
      "initial": "P",
      "tone": "accent.base",
      "what": "ordered 600 × Seal ring 40 from Katowice",
      "when": "07:48"
    },
    {
      "id": "a7",
      "who": "Grace Ltd",
      "initial": "G",
      "tone": "info.base",
      "what": "paid invoice FA-1002",
      "when": "07:15"
    },
    {
      "id": "a8",
      "who": "Nightly job",
      "initial": "N",
      "tone": "success.base",
      "what": "reconciled 1 284 stock movements",
      "when": "03:00"
    }
  ]
end

# ---- The figures the charts draw -------------------------------------------
#
# The four series are pinned: `crates/eui-client/tests/soli_e2e.rs` reads a
# bar's value out of the tree by key and asserts the donut's total, so the
# numbers here are load-bearing and the labels are not.
def erp_revenue
  {
    "line": [3, 5, 4, 8, 6, 9, 7],
    "area": [2, 4, 3, 6, 5, 8, 9],
    "bars": [4, 7, 3, 8, 5, 6],
    "mix": [5, 3, 2, 1]
  }
end

def erp_mix_labels
  ["Direct", "Search", "Social", "Mail"]
end

# The x axis of the three weekly charts. `line` and `area` carry seven days,
# `bars` six, so the labels are taken from the front of one list rather than
# kept in three.
def erp_days(n)
  ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].slice(0, n)
end

# Nine sessions of a price, `[open, high, low, close]`, oldest first: what the
# candlestick draws. The shape is the point — two sessions up, a gap down, a
# session that opened and closed at the same price, and a recovery through the
# high — so a reader can tell a wick from a body at a glance.
def erp_sessions
  [
    [42, 45, 41, 44],
    [44, 47, 43, 46],
    [46, 47, 42, 43],
    [43, 44, 40, 41],
    [41, 43, 40, 41],
    [41, 46, 41, 45],
    [45, 48, 44, 47],
    [47, 49, 45, 46],
    [46, 52, 46, 51]
  ]
end

# The rollout as the Gantt draws it: a name, the day it starts, the days it
# runs. Fifteen days, and two pairs that overlap — a plan with nothing
# overlapping in it would draw a staircase and prove nothing.
def erp_plan
  [
    {"label": "Spec", "start": 0, "span": 3},
    {"label": "Schema", "start": 2, "span": 4},
    {"label": "Import", "start": 5, "span": 5},
    {"label": "Pilot", "start": 9, "span": 4},
    {"label": "Rollout", "start": 12, "span": 3}
  ]
end

# ---- The figures the multi-series charts draw ------------------------------
#
# Three regions over six weeks, one set of numbers read four ways: grouped
# bars compare the regions week by week, the multi-line reads each region's
# trend, and the stack reads the weekly total the three make together. Using
# one set for three forms is deliberate — it is how a reader learns that the
# form is a choice about the question, not about the data.
def erp_regions
  [[4, 6, 5, 9, 7, 8], [3, 3, 5, 4, 6, 7], [2, 4, 4, 5, 5, 9]]
end

def erp_region_names
  ["North", "South", "Export"]
end

def erp_weeks
  ["W1", "W2", "W3", "W4", "W5", "W6"]
end

# Products by revenue, unsorted on purpose: the chart ranks them, and a
# ranked chart that trusts its input to arrive sorted is a chart that lies the
# first time it does not.
def erp_top_products
  [
    {"label": "Bearing 40mm", "value": 38},
    {"label": "Hex bolt M8", "value": 61},
    {"label": "Drive belt", "value": 24},
    {"label": "Gasket set", "value": 47},
    {"label": "Seal kit", "value": 15},
    {"label": "Coupling", "value": 29}
  ]
end

# Each region against its target, signed. Zero is the target, not zero sales.
def erp_variance
  [
    {"label": "North", "value": 12},
    {"label": "South", "value": -5},
    {"label": "Export", "value": 21},
    {"label": "Retail", "value": -14},
    {"label": "Trade", "value": 3}
  ]
end

# Orders taken by weekday and part of day: the grid a heatmap wants, where
# what the reader is after is where the load falls rather than any one number.
def erp_load
  [[2, 5, 9, 7, 4], [4, 8, 12, 10, 6], [1, 3, 6, 5, 2]]
end

def erp_load_rows
  ["Morning", "Afternoon", "Evening"]
end

def erp_load_cols
  ["Mon", "Tue", "Wed", "Thu", "Fri"]
end

# Lead time in days, before the new schedule and after it. Down is better
# here, which is why the chart names the two ends rather than colouring them
# good and bad.
def erp_lead_times
  [
    {"label": "Bearing 40mm", "from": 14, "to": 9},
    {"label": "Hex bolt M8", "from": 6, "to": 4},
    {"label": "Drive belt", "from": 21, "to": 18},
    {"label": "Gasket set", "from": 11, "to": 5}
  ]
end
