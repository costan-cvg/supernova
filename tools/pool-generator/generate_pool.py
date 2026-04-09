#!/usr/bin/env python3
"""
CentuRisk Risk Pool Data Generator

Generates realistic simulated risk pool datasets based on distributions
extracted from real Florida College System Risk Management Consortium data.

Default output is the app's samples/ format (pool.csv + member-sov.csv).
Delete data/centurisk.db and restart the server to auto-import generated pools.

Usage:
    python generate_pool.py                          # One medium pool -> samples/
    python generate_pool.py --pools 3 --size small   # 3 small pools -> samples/
    python generate_pool.py --size large --seed 42   # Reproducible large pool
    python generate_pool.py --format json            # Raw JSON (all asset types separate)
    python generate_pool.py --format csv             # Raw CSV per asset type
    python generate_pool.py --format xlsx            # Excel files matching source format

Workflow:
    1. python tools/pool-generator/generate_pool.py --size small --pools 2
    2. rm data/centurisk.db
    3. cargo run --bin centurisk
    The server auto-imports from samples/ on first startup.

Pool size presets:
    tiny:   3-5 members,    ~50-150 buildings
    small:  8-15 members,   ~300-800 buildings
    medium: 20-40 members,  ~1,000-3,000 buildings
    large:  50-100 members, ~3,000-8,000 buildings
    xlarge: 150-300 members, ~10,000-30,000 buildings
"""

import argparse
import json
import csv
import math
import os
import random
import string
import uuid
from collections import defaultdict
from dataclasses import dataclass, field, asdict
from datetime import date, timedelta
from typing import Optional


# ---------------------------------------------------------------------------
# Distribution tables extracted from real FCSRMC data
# ---------------------------------------------------------------------------

POOL_TYPES = [
    "College System Risk Management Consortium",
    "County Government Risk Pool",
    "Municipal Risk Management Trust",
    "School Board Risk Management Cooperative",
    "Special District Risk Sharing Pool",
    "Transit Authority Joint Insurance Fund",
    "Water District Risk Pool",
    "Housing Authority Risk Consortium",
]

POOL_REGIONS = {
    "FL": {"name": "Florida", "cities": ["Miami", "Orlando", "Tampa", "Jacksonville", "Tallahassee", "Gainesville", "Fort Lauderdale", "St. Petersburg", "Pensacola", "Sarasota", "Daytona Beach", "Fort Myers", "Palm Beach", "Lakeland", "Ocala", "Naples", "Melbourne", "Clearwater", "Port St. Lucie", "Cape Coral"]},
    "CA": {"name": "California", "cities": ["Los Angeles", "San Francisco", "San Diego", "Sacramento", "Fresno", "Oakland", "Long Beach", "Bakersfield", "Anaheim", "Riverside", "Santa Ana", "Irvine", "San Jose", "Stockton", "Modesto", "Santa Rosa", "Pasadena", "Torrance", "Burbank", "Chula Vista"]},
    "TX": {"name": "Texas", "cities": ["Houston", "Dallas", "Austin", "San Antonio", "Fort Worth", "El Paso", "Arlington", "Corpus Christi", "Plano", "Lubbock", "Irving", "Laredo", "Amarillo", "Brownsville", "McKinney", "Frisco", "Midland", "Odessa", "Round Rock", "Georgetown"]},
    "NY": {"name": "New York", "cities": ["New York", "Buffalo", "Rochester", "Syracuse", "Albany", "Yonkers", "White Plains", "Ithaca", "Utica", "New Rochelle", "Poughkeepsie", "Schenectady", "Saratoga Springs", "Binghamton", "Troy"]},
    "OH": {"name": "Ohio", "cities": ["Columbus", "Cleveland", "Cincinnati", "Toledo", "Akron", "Dayton", "Parma", "Canton", "Youngstown", "Lorain", "Springfield", "Mansfield", "Newark", "Lima", "Findlay"]},
    "PA": {"name": "Pennsylvania", "cities": ["Philadelphia", "Pittsburgh", "Allentown", "Erie", "Reading", "Scranton", "Bethlehem", "Lancaster", "Harrisburg", "York", "State College", "Wilkes-Barre", "Easton", "Chester", "Norristown"]},
    "IL": {"name": "Illinois", "cities": ["Chicago", "Aurora", "Naperville", "Joliet", "Rockford", "Springfield", "Peoria", "Elgin", "Champaign", "Bloomington", "Decatur", "Evanston", "Waukegan", "DeKalb", "Normal"]},
    "GA": {"name": "Georgia", "cities": ["Atlanta", "Augusta", "Columbus", "Savannah", "Athens", "Macon", "Roswell", "Albany", "Johns Creek", "Warner Robins", "Alpharetta", "Marietta", "Valdosta", "Smyrna", "Dalton"]},
}

# Member name templates per pool type
MEMBER_NAME_TEMPLATES = {
    "College System Risk Management Consortium": [
        "{city} Community College", "{city} State College", "{region} Technical College",
        "{city} College", "{region} Community College", "College of {city}",
        "{city} Polytechnic", "{region} State University", "{city} Institute of Technology",
    ],
    "County Government Risk Pool": [
        "{city} County", "{region} County", "County of {city}",
    ],
    "Municipal Risk Management Trust": [
        "City of {city}", "Town of {city}", "Village of {city}", "{city} Municipality",
    ],
    "School Board Risk Management Cooperative": [
        "{city} School District", "{city} Independent School District",
        "{city} Unified School District", "{region} County Schools",
    ],
    "Special District Risk Sharing Pool": [
        "{city} Special District", "{city} Utility District",
        "{city} Fire District", "{city} Park District",
    ],
    "Transit Authority Joint Insurance Fund": [
        "{city} Transit Authority", "{city} Metro",
        "{region} Regional Transit", "{city} Transportation District",
    ],
    "Water District Risk Pool": [
        "{city} Water District", "{city} Water Authority",
        "{region} Water Management District", "{city} Utilities",
    ],
    "Housing Authority Risk Consortium": [
        "{city} Housing Authority", "{city} Public Housing",
        "{region} Housing Commission",
    ],
}

CAMPUS_NAME_TEMPLATES = [
    "{city} Main Campus", "{city} North", "{city} South", "{city} East", "{city} West",
    "{city} Downtown", "{city} Central", "Advanced Technology Center",
    "Public Safety Institute", "Health Sciences Center", "Corporate Training Center",
    "Performing Arts Center", "Administration Complex", "Athletics Complex",
    "Research Park", "Innovation Hub", "{city} Annex", "Satellite Campus",
    "Community Center", "Service Center", "Operations Center", "Maintenance Facility",
]

# Building occupancy types with weights (from real data)
OCCUPANCY_TYPES = [
    ("60010 - CLASSROOMS", 0.20),
    ("60001 - ADMINISTRATION BUILDING", 0.10),
    ("60017 - SCIENCE / LABORATORY", 0.07),
    ("60052 - STORAGE BUILDING", 0.05),
    ("90008 - UTILITIES BUILDING", 0.05),
    ("60039 - PORTABLE - CLASSROOMS", 0.04),
    ("20037 - SHOP BUILDING", 0.03),
    ("60031 - LIBRARY", 0.025),
    ("60012 - TECHNICAL TRADE", 0.024),
    ("60038 - FINE ARTS", 0.02),
    ("60037 - VOCATIONAL", 0.02),
    ("90005 - STORAGE SHED", 0.02),
    ("60011 - COMMONS / STUDENT CENTER", 0.02),
    ("60026 - GYMNASIUM", 0.018),
    ("60036 - MULTIPURPOSE BUILDING", 0.017),
    ("60048 - RECREATION", 0.015),
    ("50001 - RESIDENCE HALL", 0.015),
    ("60060 - CAFETERIA", 0.014),
    ("60013 - COMPUTER CENTER", 0.013),
    ("60005 - AUDITORIUM", 0.012),
    ("30014 - MAINTENANCE SHOP", 0.01),
    ("20020 - WAREHOUSE", 0.01),
    ("80005 - PARKING GARAGE", 0.01),
    ("60022 - HEALTH CENTER", 0.008),
    ("60015 - CHILD CARE CENTER", 0.007),
    ("90001 - CENTRAL PLANT", 0.006),
    ("60050 - SECURITY", 0.005),
    ("80001 - CARPORT", 0.005),
    ("60044 - WELDING SHOP", 0.004),
    ("20001 - AUTO SHOP", 0.004),
]

# ISO Construction classes with weights
ISO_CONSTRUCTION = [
    ("4 - MASONRY NON COMBUSTIBLE", 0.305),
    ("6 - FIRE RESISTIVE", 0.157),
    ("5 - MODIFIED FIRE RESISTIVE", 0.133),
    ("UNK - UNKNOWN", 0.126),
    ("3 - NON COMBUSTIBLE", 0.090),
    ("2 - JOISTED MASONRY", 0.085),
    ("1 - FRAME/COMBUSTIBLE", 0.051),
    ("N - NOT APPLICABLE", 0.044),
    ("9 - SUPERIOR MASONRY NON COMBUSTIBLE", 0.005),
    ("8 - SUPERIOR NON COMBUSTIBLE", 0.004),
]

FRAME_TYPES = [
    ("ST - STEEL", 0.32),
    ("RC - REINFORCED CONCRETE", 0.16),
    ("UNK - UNKNOWN", 0.15),
    ("FPS - FIRE-PROOFED STEEL", 0.14),
    ("PES - PRE-ENGINEERED STEEL", 0.09),
    ("JM - JOISTED MASONRY", 0.09),
    ("WD - WOOD", 0.05),
]

ROOFING_TYPES = [
    ("D - BUILT-UP SMOOTH", 0.70),
    ("A - METAL", 0.16),
    ("J - SINGLE MEMBRANE", 0.10),
    ("C - ASPHALT SHINGLES", 0.02),
    ("N - CLAY TILE", 0.01),
    ("E - BUILT-UP TAR & GRAVEL", 0.01),
]

EXTERIOR_WALLS = [
    ("OA - BRICK ON CONCRETE BLOCK", 0.47),
    ("J - PRECAST CONCRETE PANEL", 0.27),
    ("D - STUCCO ON MASONRY", 0.07),
    ("IA - DECORATIVE CONCRETE BLOCK", 0.05),
    ("LA - REINFORCED CONCRETE", 0.05),
    ("K - TILT-UP CONCRETE PANEL", 0.04),
    ("T - GLASS METAL CURTAIN", 0.02),
    ("G - METAL SIDING ON GIRTS", 0.02),
    ("Q - STONE ON MASONRY", 0.01),
]

FLOOD_ZONES = [
    ("X", 0.75), ("X500", 0.075), ("AE", 0.07), ("B", 0.055),
    ("A", 0.03), ("AH", 0.01), ("A4", 0.005), ("VE", 0.005),
]

CONDITIONS = [("A - AVERAGE", 0.97), ("E - EXCELLENT", 0.02), ("N - NEW", 0.01)]

VALUATION_SOURCES = [
    ("4 - TREND STATEMENT OF VALUE", 0.68),
    ("2 - TREND APPRAISAL", 0.26),
    ("7 - MEMBER SUPPLIED", 0.06),
]

# Vehicle data
VEHICLE_CLASSES = [
    ("2600 - LICENSED VEHICLES", 0.82),
    ("2802 - TRAILERS", 0.17),
    ("2600A - LICENSED VEHICLES (ACV)", 0.005),
    ("2801 - GOLF CARTS", 0.003),
    ("2802A - TRAILERS (ACV)", 0.002),
]

VEHICLE_MAKES = [
    ("FORD", 0.40), ("CHEVROLET", 0.10), ("NISSAN", 0.05), ("DODGE", 0.04),
    ("FREIGHTLINER", 0.03), ("GMC", 0.03), ("TOYOTA", 0.02), ("HONDA", 0.02),
    ("INTERNATIONAL", 0.015), ("RAM", 0.01), ("JEEP", 0.01), ("HYUNDAI", 0.01),
    ("KIA", 0.01), ("ISUZU", 0.008), ("MITSUBISHI", 0.005), ("UTILITY", 0.05),
    ("CARRY-ON", 0.03), ("PJ", 0.02), ("LOAD TRAIL", 0.015),
]

VEHICLE_DESCRIPTIONS = [
    "SEDAN", "SUV", "PICKUP TRUCK", "VAN", "CARGO VAN", "BUS",
    "UTILITY TRAILER", "FLATBED TRAILER", "ENCLOSED TRAILER",
    "BOX TRUCK", "DUMP TRUCK", "MAINTENANCE TRUCK", "GOLF CART",
    "FORKLIFT", "SHUTTLE BUS", "MINIVAN", "PASSENGER VAN",
]

# Equipment data
EQUIPMENT_CLASSES = [
    ("2801 - GOLF CARTS", 0.48),
    ("2803 - UTILITY EQUIPMENT", 0.17),
    ("2800 - MOWERS", 0.13),
    ("2100 - DRONES", 0.11),
    ("1101 - WATERCRAFT", 0.10),
    ("2805 - AIRCRAFT-EDUCATION", 0.01),
]

EQUIPMENT_DESCRIPTIONS = {
    "2801 - GOLF CARTS": ["GOLF CART", "EZ-GO GOLF CART", "CLUB CAR", "YAMAHA GOLF CART", "UTILITY CART"],
    "2803 - UTILITY EQUIPMENT": ["GENERATOR", "COMPRESSOR", "WELDER", "PRESSURE WASHER", "CHAINSAW", "FORKLIFT", "SKID STEER"],
    "2800 - MOWERS": ["RIDING MOWER", "ZERO-TURN MOWER", "WALK-BEHIND MOWER", "TORO MOWER", "JOHN DEERE MOWER"],
    "2100 - DRONES": ["DJI MAVIC", "DJI PHANTOM", "DRONE", "QUADCOPTER", "DJI INSPIRE"],
    "1101 - WATERCRAFT": ["UTILITY BOAT", "PONTOON BOAT", "JON BOAT", "KAYAK", "CANOE"],
    "2805 - AIRCRAFT-EDUCATION": ["CESSNA 172", "PIPER CHEROKEE", "CESSNA 152", "FLIGHT SIMULATOR"],
}

# PITO descriptions
PITO_DESCRIPTIONS = [
    "WALKWAYS AND LIGHTING", "FENCING AND GATES", "PARKING LOT",
    "SIGNAGE AND MONUMENT", "ATHLETIC FIELD", "RUNNING TRACK",
    "TENNIS COURTS", "BASKETBALL COURTS", "SWIMMING POOL",
    "PLAYGROUND EQUIPMENT", "BLEACHERS", "FLAGPOLE",
    "IRRIGATION SYSTEM", "LANDSCAPE AND HARDSCAPE",
    "RETENTION POND SYSTEM", "WATER TOWER", "FUEL STATION",
    "BOAT DOCK", "COVERED WALKWAY", "CANOPY STRUCTURE",
    "SOLAR PANEL ARRAY", "ANTENNA TOWER", "CELL TOWER LEASE",
]


# ---------------------------------------------------------------------------
# Weighted random selection helper
# ---------------------------------------------------------------------------

def weighted_choice(choices: list[tuple[str, float]]) -> str:
    items, weights = zip(*choices)
    return random.choices(items, weights=weights, k=1)[0]


def weighted_choices(choices: list[tuple[str, float]], k: int) -> list[str]:
    items, weights = zip(*choices)
    return random.choices(items, weights=weights, k=k)


# ---------------------------------------------------------------------------
# Value generators with realistic distributions
# ---------------------------------------------------------------------------

def gen_sqft(occupancy: str) -> int:
    """Generate realistic square footage based on occupancy type. Log-normal distribution."""
    base = {
        "60010": (8000, 1.0),    # classrooms
        "60001": (12000, 0.9),   # admin
        "60017": (15000, 0.8),   # science lab
        "60052": (3000, 1.2),    # storage
        "90008": (2000, 1.1),    # utilities
        "60039": (1500, 0.4),    # portables
        "20037": (5000, 0.8),    # shop
        "60031": (25000, 0.7),   # library
        "60026": (30000, 0.6),   # gym
        "60011": (20000, 0.7),   # commons
        "50001": (40000, 0.8),   # residence hall
        "80005": (80000, 0.5),   # parking garage
    }
    code = occupancy.split(" - ")[0] if " - " in occupancy else "60010"
    median, sigma = base.get(code, (9800, 1.0))
    val = random.lognormvariate(math.log(median), sigma)
    return max(100, min(900000, int(val)))


def gen_construction_year() -> int:
    """Construction years with realistic distribution. Peak around 1990-2010."""
    r = random.random()
    if r < 0.05:
        return random.randint(1902, 1960)
    elif r < 0.15:
        return random.randint(1960, 1980)
    elif r < 0.40:
        return random.randint(1980, 2000)
    elif r < 0.75:
        return random.randint(2000, 2015)
    else:
        return random.randint(2015, 2025)


def gen_stories(sqft: int) -> int:
    """Number of stories correlated with square footage."""
    if sqft < 2000:
        return 1
    elif sqft < 10000:
        return random.choices([1, 2], weights=[0.8, 0.2])[0]
    elif sqft < 30000:
        return random.choices([1, 2, 3], weights=[0.5, 0.35, 0.15])[0]
    elif sqft < 80000:
        return random.choices([2, 3, 4, 5], weights=[0.4, 0.35, 0.15, 0.1])[0]
    else:
        return random.choices([3, 4, 5, 6, 7, 8], weights=[0.3, 0.3, 0.2, 0.1, 0.05, 0.05])[0]


def gen_building_rcn(sqft: int, year_built: int, iso_class: str) -> float:
    """Generate replacement cost new. Based on $/sqft with modifiers."""
    # Base cost per sqft varies by construction class
    base_cost = {
        "6 - FIRE RESISTIVE": random.uniform(180, 350),
        "5 - MODIFIED FIRE RESISTIVE": random.uniform(150, 300),
        "4 - MASONRY NON COMBUSTIBLE": random.uniform(130, 250),
        "9 - SUPERIOR MASONRY NON COMBUSTIBLE": random.uniform(200, 400),
        "8 - SUPERIOR NON COMBUSTIBLE": random.uniform(200, 380),
        "3 - NON COMBUSTIBLE": random.uniform(120, 220),
        "2 - JOISTED MASONRY": random.uniform(100, 200),
        "1 - FRAME/COMBUSTIBLE": random.uniform(80, 180),
    }.get(iso_class, random.uniform(100, 250))

    # Age modifier: newer buildings cost more to replace
    age = 2025 - year_built
    age_mod = 1.0 + max(0, (30 - age) * 0.005)

    rcn = sqft * base_cost * age_mod
    # Add some noise
    rcn *= random.uniform(0.85, 1.15)
    return round(rcn, 2)


def gen_acv_ratio(year_built: int) -> float:
    """ACV as percentage of RCN, based on age."""
    age = 2025 - year_built
    # Depreciation curve
    if age <= 5:
        return random.uniform(0.90, 0.98)
    elif age <= 15:
        return random.uniform(0.75, 0.90)
    elif age <= 30:
        return random.uniform(0.55, 0.80)
    elif age <= 50:
        return random.uniform(0.40, 0.65)
    else:
        return random.uniform(0.30, 0.55)


def gen_contents_ratio() -> float:
    """Contents value as ratio of RCN."""
    return random.uniform(0.05, 0.20)


def gen_vehicle_rcn(vehicle_class: str, model_year: int) -> float:
    """Vehicle replacement cost."""
    base = {
        "2600 - LICENSED VEHICLES": random.uniform(15000, 65000),
        "2802 - TRAILERS": random.uniform(2000, 25000),
        "2600A - LICENSED VEHICLES (ACV)": random.uniform(8000, 40000),
        "2801 - GOLF CARTS": random.uniform(5000, 12000),
        "2802A - TRAILERS (ACV)": random.uniform(1500, 15000),
    }.get(vehicle_class, random.uniform(10000, 40000))
    age = 2025 - model_year
    depreciation = max(0.15, 1.0 - age * 0.08)
    return round(base * depreciation * random.uniform(0.85, 1.15), 2)


def gen_equipment_rcn(eq_class: str) -> float:
    """Equipment replacement cost."""
    ranges = {
        "2801 - GOLF CARTS": (3000, 15000),
        "2803 - UTILITY EQUIPMENT": (500, 50000),
        "2800 - MOWERS": (2000, 25000),
        "2100 - DRONES": (500, 15000),
        "1101 - WATERCRAFT": (2000, 60000),
        "2805 - AIRCRAFT-EDUCATION": (50000, 900000),
    }
    lo, hi = ranges.get(eq_class, (1000, 30000))
    return round(random.uniform(lo, hi), 2)


def gen_pito_rcn() -> float:
    """Property in the open replacement cost. Log-normal distribution."""
    return round(random.lognormvariate(math.log(150000), 1.2), 2)


def gen_zipcode(state: str) -> str:
    """Generate a plausible zip code for the state."""
    prefixes = {
        "FL": range(32000, 34999), "CA": range(90000, 96699),
        "TX": range(73301, 79999), "NY": range(10000, 14999),
        "OH": range(43000, 45999), "PA": range(15000, 19699),
        "IL": range(60000, 62999), "GA": range(30000, 31999),
    }
    r = prefixes.get(state, range(10000, 99999))
    return str(random.randint(r.start, r.stop))


def gen_lat_lon(state: str) -> tuple[float, float]:
    """Generate plausible lat/lon for state centroid area."""
    centers = {
        "FL": (28.0, -82.0, 2.5, 3.0), "CA": (36.5, -119.5, 3.0, 3.0),
        "TX": (31.0, -99.0, 3.0, 4.0), "NY": (42.5, -75.5, 1.5, 2.0),
        "OH": (40.0, -82.5, 1.0, 1.5), "PA": (40.8, -77.5, 0.8, 2.0),
        "IL": (40.0, -89.0, 2.0, 1.5), "GA": (33.0, -83.5, 1.5, 1.5),
    }
    lat_c, lon_c, lat_r, lon_r = centers.get(state, (39.0, -98.0, 5.0, 10.0))
    return (
        round(lat_c + random.uniform(-lat_r, lat_r), 6),
        round(lon_c + random.uniform(-lon_r, lon_r), 6),
    )


def gen_inspection_date(year_built: int) -> str:
    """Generate a plausible inspection date."""
    base = date(max(year_built, 2015), 1, 1)
    offset = random.randint(0, (date(2025, 12, 31) - base).days)
    return (base + timedelta(days=offset)).isoformat()


def gen_vin() -> str:
    """Generate a plausible VIN-like string."""
    chars = string.ascii_uppercase.replace("I", "").replace("O", "").replace("Q", "") + string.digits
    return "".join(random.choices(chars, k=17))


def gen_member_code(name: str) -> str:
    """Generate a short member code from name."""
    words = name.upper().split()
    if len(words) >= 3:
        code = "".join(w[0] for w in words[:4])
    else:
        code = words[0][:4]
    return code[:5]


# ---------------------------------------------------------------------------
# Pool Generator
# ---------------------------------------------------------------------------

@dataclass
class PoolConfig:
    """Configuration for a generated risk pool."""
    pool_name: str = ""
    pool_type: str = ""
    state: str = "FL"
    num_members: int = 0
    # Ratio ranges - how many of each asset type per campus
    buildings_per_campus: tuple[int, int] = (3, 15)
    vehicles_per_member: tuple[int, int] = (10, 100)
    equipment_per_member: tuple[int, int] = (10, 80)
    pito_per_campus: tuple[int, int] = (2, 10)
    campuses_per_member: tuple[int, int] = (2, 12)
    seed: Optional[int] = None


SIZE_PRESETS = {
    "tiny": {
        "num_members": (3, 5),
        "campuses_per_member": (1, 4),
        "buildings_per_campus": (2, 8),
        "vehicles_per_member": (5, 30),
        "equipment_per_member": (5, 20),
        "pito_per_campus": (1, 5),
    },
    "small": {
        "num_members": (8, 15),
        "campuses_per_member": (2, 8),
        "buildings_per_campus": (3, 12),
        "vehicles_per_member": (10, 60),
        "equipment_per_member": (10, 50),
        "pito_per_campus": (2, 8),
    },
    "medium": {
        "num_members": (20, 40),
        "campuses_per_member": (3, 10),
        "buildings_per_campus": (4, 15),
        "vehicles_per_member": (20, 100),
        "equipment_per_member": (15, 80),
        "pito_per_campus": (3, 10),
    },
    "large": {
        "num_members": (50, 100),
        "campuses_per_member": (4, 14),
        "buildings_per_campus": (5, 18),
        "vehicles_per_member": (30, 150),
        "equipment_per_member": (20, 100),
        "pito_per_campus": (3, 12),
    },
    "xlarge": {
        "num_members": (150, 300),
        "campuses_per_member": (5, 17),
        "buildings_per_campus": (5, 20),
        "vehicles_per_member": (40, 200),
        "equipment_per_member": (30, 120),
        "pito_per_campus": (4, 15),
    },
}


def make_config(size: str = "medium", num_members: Optional[int] = None,
                seed: Optional[int] = None) -> PoolConfig:
    """Create a pool config from a size preset with optional overrides."""
    preset = SIZE_PRESETS[size]
    rng = random.Random(seed)

    state = rng.choice(list(POOL_REGIONS.keys()))
    pool_type = rng.choice(POOL_TYPES)
    region_name = POOL_REGIONS[state]["name"]
    pool_name = f"{region_name} {pool_type}"

    member_count = num_members or rng.randint(*preset["num_members"])

    return PoolConfig(
        pool_name=pool_name,
        pool_type=pool_type,
        state=state,
        num_members=member_count,
        campuses_per_member=preset["campuses_per_member"],
        buildings_per_campus=preset["buildings_per_campus"],
        vehicles_per_member=preset["vehicles_per_member"],
        equipment_per_member=preset["equipment_per_member"],
        pito_per_campus=preset["pito_per_campus"],
        seed=seed,
    )


def generate_pool(config: PoolConfig) -> dict:
    """Generate a complete risk pool dataset."""
    if config.seed is not None:
        random.seed(config.seed)

    pool_id = str(uuid.uuid4())[:8].upper()
    state = config.state
    region = POOL_REGIONS[state]
    cities = list(region["cities"])

    # Pick member name templates based on pool type
    templates = MEMBER_NAME_TEMPLATES.get(config.pool_type, MEMBER_NAME_TEMPLATES["County Government Risk Pool"])

    # ---- Members ----
    members = []
    used_cities = set()
    for i in range(config.num_members):
        city = random.choice(cities)
        # Avoid exact duplicate names but allow city reuse
        template = random.choice(templates)
        name = template.format(city=city, region=region["name"])
        suffix = f" {i+1}" if name in used_cities else ""
        name += suffix
        used_cities.add(name)

        code = gen_member_code(name)
        if any(m["member_code"] == code for m in members):
            code = code + str(i)

        members.append({
            "member_number": f"M{i+1:04d}",
            "member_code": code,
            "member_name": name,
            "entity_name": config.pool_name,
        })

    # ---- Campuses ----
    campuses = []
    campus_counter = 0
    for member in members:
        num_campuses = random.randint(*config.campuses_per_member)
        member_city = member["member_name"].split(" ")[0] if "of" not in member["member_name"].lower() else member["member_name"].split("of ")[-1].split(" ")[0]

        for j in range(num_campuses):
            campus_counter += 1
            if j == 0:
                campus_name = f"{member_city} Main Campus"
            else:
                template = random.choice(CAMPUS_NAME_TEMPLATES)
                campus_name = template.format(city=member_city)

            campuses.append({
                "campus_number": f"C{campus_counter:05d}",
                "campus_name": campus_name,
                "description": campus_name,
                "member_code": member["member_code"],
                "member_name": member["member_name"],
            })

    # ---- Buildings ----
    buildings = []
    building_valuations = []
    asset_counter = 0

    for campus in campuses:
        num_buildings = random.randint(*config.buildings_per_campus)
        lat_base, lon_base = gen_lat_lon(state)

        for k in range(num_buildings):
            asset_counter += 1
            occupancy = weighted_choice(OCCUPANCY_TYPES)
            iso_class = weighted_choice(ISO_CONSTRUCTION)
            sqft = gen_sqft(occupancy)
            year_built = gen_construction_year()
            stories = gen_stories(sqft)
            condition = weighted_choice(CONDITIONS)
            frame_type = weighted_choice(FRAME_TYPES)
            flood_zone = weighted_choice(FLOOD_ZONES)

            has_sprinklers = random.random() < 0.61
            has_fire_alarms = random.random() < 0.88

            asset_id = f"{campus['member_code']}{campus['campus_number'][-3:]}B{k+1:03d}"

            # Generate name from occupancy
            occ_name = occupancy.split(" - ")[1] if " - " in occupancy else occupancy
            bldg_name = f"{occ_name} {k+1}" if num_buildings > 1 else occ_name

            lat = round(lat_base + random.uniform(-0.01, 0.01), 6)
            lon = round(lon_base + random.uniform(-0.01, 0.01), 6)

            building = {
                "asset_number": asset_id,
                "member_campus": f"{campus['member_code']} - {campus['member_name']}",
                "site": f"{campus['campus_number']} - {campus['campus_name']}",
                "building_number": str(k + 1),
                "building_name": bldg_name,
                "class": "3000 - BUILDINGS",
                "category": "300 - BUILDINGS",
                "inspection_date": gen_inspection_date(year_built),
                "address_1": f"{random.randint(100, 9999)} {random.choice(['Main', 'College', 'Campus', 'University', 'Oak', 'Pine', 'Elm', 'Park', 'Lake', 'Center'])} {random.choice(['Street', 'Road', 'Avenue', 'Boulevard', 'Drive', 'Lane', 'Way'])}",
                "zip_code": gen_zipcode(state),
                "city": campus["campus_name"].split(" ")[0],
                "state": state,
                "county": "",
                "condition": condition,
                "acquisition_date": f"01/01/{year_built}",
                "construction_year": year_built,
                "total_sqft": sqft,
                "number_of_stories": stories,
                "iso_construction_class": iso_class,
                "occupancy": occupancy,
                "frame_type": frame_type,
                "flood_zone": flood_zone,
                "latitude": lat,
                "longitude": lon,
                "sprinklers": "YES" if has_sprinklers else "NO",
                "sprinkler_type_1": random.choice(["WET PIPE", "DRY PIPE", "PRE-ACTION"]) if has_sprinklers else "",
                "sprinkler_type_1_pct": random.choice([80, 90, 100]) if has_sprinklers else 0,
                "fire_alarms": "YES" if has_fire_alarms else "NO",
                "fire_alarm_type_1": random.choice(["SMOKE", "HEAT", "COMBINATION"]) if has_fire_alarms else "",
                "fire_alarm_type_1_pct": random.choice([90, 100]) if has_fire_alarms else 0,
                "roofing_type_1": weighted_choice(ROOFING_TYPES),
                "exterior_walls_1": weighted_choice(EXTERIOR_WALLS),
                "foundation_type_1": "1 - CONCRETE FOUNDATION WALLS",
                "ownership": random.choices(["1 - OWNED", "0 - LEASED"], weights=[0.92, 0.08])[0],
                "is_insured": "YES",
                "vacant": "NO",
                "year_last_roof_upgrade": random.choice([None, year_built + random.randint(10, 30)]) if year_built < 2000 else None,
                "year_last_electrical_upgrade": random.choice([None, year_built + random.randint(15, 35)]) if year_built < 1995 else None,
            }
            buildings.append(building)

            # Valuation
            rcn = gen_building_rcn(sqft, year_built, iso_class)
            acv_ratio = gen_acv_ratio(year_built)
            contents = rcn * gen_contents_ratio()

            valuation = {
                "asset_number": asset_id,
                "valuation_source": weighted_choice(VALUATION_SOURCES),
                "as_of_date": f"11/05/{random.choice([2024, 2025])}",
                "replacement_cost_new": round(rcn, 2),
                "replacement_cost_new_exclusion": 0,
                "actual_cash_value": round(rcn * acv_ratio, 2),
                "actual_cash_value_exclusion": 0,
                "reproduction_cost": 0,
                "reproduction_cost_exclusion": 0,
                "modeled_contents_value": round(contents, 2),
                "edp": 0,
                "comments": f"{random.choice([2024, 2025])} TREND",
                "rcn_per_sqft": round(rcn / max(sqft, 1), 2),
                "acv_per_sqft": round(rcn * acv_ratio / max(sqft, 1), 2),
                "total_insurable_value": round(rcn + contents, 2),
            }
            building_valuations.append(valuation)

    # ---- Vehicles ----
    vehicles = []
    vehicle_valuations = []
    vehicle_counter = 0

    for member in members:
        num_vehicles = random.randint(*config.vehicles_per_member)
        for v in range(num_vehicles):
            vehicle_counter += 1
            vclass = weighted_choice(VEHICLE_CLASSES)
            make = weighted_choice(VEHICLE_MAKES)
            model_year = random.randint(1995, 2025)
            description = random.choice(VEHICLE_DESCRIPTIONS)

            vid = f"{member['member_code']}V{vehicle_counter:05d}"
            vehicle = {
                "asset_number": vid,
                "member_campus": f"{member['member_code']} - {member['member_name']}",
                "description": f"{make} - {description}",
                "class": vclass,
                "category": "600 - LICENSED VEHICLES",
                "acquisition_date": f"01/01/{model_year}",
                "quantity": 1,
                "condition": weighted_choice(CONDITIONS),
                "make": make,
                "model": description,
                "model_year": model_year,
                "vin": gen_vin(),
                "ownership": "1 - OWNED",
                "is_insured": "YES",
            }
            vehicles.append(vehicle)

            rcn = gen_vehicle_rcn(vclass, model_year)
            vehicle_valuations.append({
                "asset_number": vid,
                "valuation_source": "7 - MEMBER SUPPLIED",
                "as_of_date": f"01/01/{random.choice([2024, 2025])}",
                "replacement_cost_new": rcn,
                "replacement_cost_new_exclusion": 0,
                "actual_cash_value": round(rcn * gen_acv_ratio(model_year), 2),
                "actual_cash_value_exclusion": 0,
                "total_insurable_value": rcn,
            })

    # ---- Movable Equipment ----
    equipment = []
    equipment_valuations = []
    eq_counter = 0

    for member in members:
        num_eq = random.randint(*config.equipment_per_member)
        for e in range(num_eq):
            eq_counter += 1
            eq_class = weighted_choice(EQUIPMENT_CLASSES)
            descriptions = EQUIPMENT_DESCRIPTIONS.get(eq_class, ["EQUIPMENT"])
            description = random.choice(descriptions)

            eid = f"{member['member_code']}E{eq_counter:05d}"
            eq = {
                "asset_number": eid,
                "member_campus": f"{member['member_code']} - {member['member_name']}",
                "class": eq_class,
                "category": "500 - MOVABLE EQUIPMENT",
                "description": description,
                "quantity": 1,
                "condition": weighted_choice(CONDITIONS),
                "acquisition_date": f"01/01/{random.randint(2010, 2025)}",
                "ownership": "1 - OWNED",
                "is_insured": "YES",
            }
            equipment.append(eq)

            rcn = gen_equipment_rcn(eq_class)
            equipment_valuations.append({
                "asset_number": eid,
                "valuation_source": "7 - MEMBER SUPPLIED",
                "as_of_date": f"01/01/{random.choice([2024, 2025])}",
                "replacement_cost_new": rcn,
                "replacement_cost_new_exclusion": 0,
                "actual_cash_value": round(rcn * random.uniform(0.4, 0.9), 2),
                "actual_cash_value_exclusion": 0,
                "total_insurable_value": rcn,
            })

    # ---- Property in the Open (PITO) ----
    pito = []
    pito_valuations = []
    pito_counter = 0

    for campus in campuses:
        num_pito = random.randint(*config.pito_per_campus)
        for p in range(num_pito):
            pito_counter += 1
            description = random.choice(PITO_DESCRIPTIONS)
            pid = f"{campus['member_code']}{campus['campus_number'][-3:]}P{p+1:03d}"

            pito_item = {
                "asset_number": pid,
                "member_campus": f"{campus['member_code']} - {campus['campus_name']}",
                "site": f"{campus['campus_number']} - {campus['campus_name']}",
                "class": "2900 - LAND IMPROVEMENTS",
                "category": "200 - PROPERTY IN THE OPEN",
                "description": f"{description} {campus['campus_name']}",
                "inspection_date": gen_inspection_date(2015),
                "acquisition_date": f"01/01/{random.randint(1990, 2024)}",
                "condition": weighted_choice(CONDITIONS),
                "construction_year": random.randint(1990, 2024),
                "quantity": 1,
                "ownership": "1 - OWNED",
                "is_insured": "YES",
            }
            pito.append(pito_item)

            rcn = gen_pito_rcn()
            pito_valuations.append({
                "asset_number": pid,
                "valuation_source": weighted_choice(VALUATION_SOURCES),
                "as_of_date": f"01/01/{random.choice([2024, 2025])}",
                "replacement_cost_new": rcn,
                "replacement_cost_new_exclusion": 0,
                "actual_cash_value": round(rcn * random.uniform(0.5, 0.85), 2),
                "actual_cash_value_exclusion": 0,
                "total_insurable_value": rcn,
            })

    # ---- Summary stats ----
    total_building_tiv = sum(v["total_insurable_value"] for v in building_valuations)
    total_vehicle_tiv = sum(v["total_insurable_value"] for v in vehicle_valuations)
    total_equipment_tiv = sum(v["total_insurable_value"] for v in equipment_valuations)
    total_pito_tiv = sum(v["total_insurable_value"] for v in pito_valuations)

    summary = {
        "pool_id": pool_id,
        "pool_name": config.pool_name,
        "pool_type": config.pool_type,
        "state": state,
        "members": len(members),
        "campuses": len(campuses),
        "buildings": len(buildings),
        "vehicles": len(vehicles),
        "equipment": len(equipment),
        "pito": len(pito),
        "total_assets": len(buildings) + len(vehicles) + len(equipment) + len(pito),
        "total_insurable_value": round(total_building_tiv + total_vehicle_tiv + total_equipment_tiv + total_pito_tiv, 2),
        "building_tiv": round(total_building_tiv, 2),
        "vehicle_tiv": round(total_vehicle_tiv, 2),
        "equipment_tiv": round(total_equipment_tiv, 2),
        "pito_tiv": round(total_pito_tiv, 2),
    }

    return {
        "summary": summary,
        "members": members,
        "campuses": campuses,
        "buildings": buildings,
        "building_valuations": building_valuations,
        "vehicles": vehicles,
        "vehicle_valuations": vehicle_valuations,
        "equipment": equipment,
        "equipment_valuations": equipment_valuations,
        "pito": pito,
        "pito_valuations": pito_valuations,
    }


# ---------------------------------------------------------------------------
# Output writers
# ---------------------------------------------------------------------------

def write_json(pool_data: dict, output_dir: str):
    """Write pool data as JSON files."""
    os.makedirs(output_dir, exist_ok=True)

    # Summary
    with open(os.path.join(output_dir, "summary.json"), "w") as f:
        json.dump(pool_data["summary"], f, indent=2)

    # Each dataset
    for key in ["members", "campuses", "buildings", "building_valuations",
                 "vehicles", "vehicle_valuations", "equipment",
                 "equipment_valuations", "pito", "pito_valuations"]:
        with open(os.path.join(output_dir, f"{key}.json"), "w") as f:
            json.dump(pool_data[key], f, indent=2)


def write_csv(pool_data: dict, output_dir: str):
    """Write pool data as CSV files."""
    os.makedirs(output_dir, exist_ok=True)

    # Summary
    with open(os.path.join(output_dir, "summary.json"), "w") as f:
        json.dump(pool_data["summary"], f, indent=2)

    for key in ["members", "campuses", "buildings", "building_valuations",
                 "vehicles", "vehicle_valuations", "equipment",
                 "equipment_valuations", "pito", "pito_valuations"]:
        records = pool_data[key]
        if not records:
            continue
        filepath = os.path.join(output_dir, f"{key}.csv")
        with open(filepath, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=records[0].keys())
            writer.writeheader()
            writer.writerows(records)


def write_samples(pool_data: dict, output_dir: str):
    """Write pool data in the CentuRisk samples/ format.

    Produces:
      <output_dir>/pool.csv             — pool_name, member_name, member_contact_email
      <output_dir>/<member-slug>-sov.csv — one per member, SovRow format

    The SovRow CSV columns match what the server's onboard_from_samples()
    and POST /api/onboard endpoints expect:
      asset_type,building_name,address,city,state,zip_code,year_built,
      construction_class,occupancy,sq_footage,stories,replacement_cost,
      sprinkler,roof_type,contents_value

    Drop the output into ./samples/ and restart the server — it auto-imports.
    """
    os.makedirs(output_dir, exist_ok=True)

    summary = pool_data["summary"]
    members = pool_data["members"]
    buildings = pool_data["buildings"]
    building_vals = {v["asset_number"]: v for v in pool_data["building_valuations"]}
    vehicles = pool_data["vehicles"]
    vehicle_vals = {v["asset_number"]: v for v in pool_data["vehicle_valuations"]}
    equipment = pool_data["equipment"]
    equipment_vals = {v["asset_number"]: v for v in pool_data["equipment_valuations"]}
    pito = pool_data["pito"]
    pito_vals = {v["asset_number"]: v for v in pool_data["pito_valuations"]}

    pool_name = summary["pool_name"]

    # ── pool.csv ──────────────────────────────────────────────────────
    with open(os.path.join(output_dir, "pool.csv"), "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["pool_name", "member_name", "member_contact_email"])
        for m in members:
            slug = m["member_name"].lower().replace(" ", "-").replace("of-", "")
            writer.writerow([pool_name, m["member_name"], f"facilities@{slug}.gov"])

    # ── per-member SOV CSVs ───────────────────────────────────────────
    SOV_COLUMNS = [
        "asset_type", "building_name", "address", "city", "state", "zip_code",
        "year_built", "construction_class", "occupancy", "sq_footage", "stories",
        "replacement_cost", "sprinkler", "roof_type", "contents_value",
    ]

    # Index assets by member code
    member_buildings = defaultdict(list)
    for b in buildings:
        code = b["member_campus"].split(" - ")[0].strip()
        member_buildings[code].append(b)

    member_vehicles = defaultdict(list)
    for v in vehicles:
        code = v["member_campus"].split(" - ")[0].strip()
        member_vehicles[code].append(v)

    member_equipment = defaultdict(list)
    for e in equipment:
        code = e["member_campus"].split(" - ")[0].strip()
        member_equipment[code].append(e)

    member_pito = defaultdict(list)
    for p in pito:
        code = p["member_campus"].split(" - ")[0].strip()
        member_pito[code].append(p)

    for idx, m in enumerate(members):
        slug = m["member_name"].lower().replace(" ", "-").replace("of-", "")
        sov_path = os.path.join(output_dir, f"{slug}-sov.csv")

        with open(sov_path, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=SOV_COLUMNS, extrasaction="ignore")
            writer.writeheader()

            code = m["member_code"]

            # Buildings
            for b in member_buildings.get(code, []):
                val = building_vals.get(b["asset_number"], {})
                writer.writerow({
                    "asset_type": "Building",
                    "building_name": b.get("building_name", ""),
                    "address": b.get("address_1", ""),
                    "city": b.get("city", ""),
                    "state": b.get("state", ""),
                    "zip_code": b.get("zip_code", ""),
                    "year_built": b.get("construction_year", ""),
                    "construction_class": b.get("iso_construction_class", ""),
                    "occupancy": b.get("occupancy", ""),
                    "sq_footage": b.get("total_sqft", ""),
                    "stories": b.get("number_of_stories", ""),
                    "replacement_cost": int(val.get("replacement_cost_new", 0)),
                    "sprinkler": "true" if b.get("sprinklers") == "YES" else "false",
                    "roof_type": b.get("roofing_type_1", ""),
                    "contents_value": int(val.get("modeled_contents_value", 0)),
                })

            # Vehicles
            for v in member_vehicles.get(code, []):
                val = vehicle_vals.get(v["asset_number"], {})
                writer.writerow({
                    "asset_type": "LicensedVehicle",
                    "building_name": v.get("description", ""),
                    "address": "",
                    "city": "",
                    "state": b.get("state", "") if member_buildings.get(code) else summary.get("state", ""),
                    "zip_code": "",
                    "year_built": v.get("model_year", ""),
                    "construction_class": "",
                    "occupancy": "",
                    "sq_footage": "",
                    "stories": "",
                    "replacement_cost": int(val.get("replacement_cost_new", 0)),
                    "sprinkler": "",
                    "roof_type": "",
                    "contents_value": "",
                })

            # Movable Equipment
            for e in member_equipment.get(code, []):
                val = equipment_vals.get(e["asset_number"], {})
                writer.writerow({
                    "asset_type": "MovableEquipment",
                    "building_name": e.get("description", ""),
                    "address": "",
                    "city": "",
                    "state": "",
                    "zip_code": "",
                    "year_built": "",
                    "construction_class": "",
                    "occupancy": e.get("class", ""),
                    "sq_footage": "",
                    "stories": "",
                    "replacement_cost": int(val.get("replacement_cost_new", 0)),
                    "sprinkler": "",
                    "roof_type": "",
                    "contents_value": "",
                })

            # Property in the Open
            for p in member_pito.get(code, []):
                val = pito_vals.get(p["asset_number"], {})
                writer.writerow({
                    "asset_type": "PropertyInTheOpen",
                    "building_name": p.get("description", ""),
                    "address": "",
                    "city": "",
                    "state": "",
                    "zip_code": "",
                    "year_built": str(p.get("construction_year", "")),
                    "construction_class": "",
                    "occupancy": p.get("class", ""),
                    "sq_footage": "",
                    "stories": "",
                    "replacement_cost": int(val.get("replacement_cost_new", 0)),
                    "sprinkler": "",
                    "roof_type": "",
                    "contents_value": "",
                })

    # Also write summary.json for reference
    with open(os.path.join(output_dir, "summary.json"), "w") as f:
        json.dump(summary, f, indent=2)

    # Write onboard.json — ready to POST to /api/onboard
    onboard_payload = {
        "pool_name": pool_name,
        "members": [],
    }
    for idx, m in enumerate(members):
        slug = m["member_name"].lower().replace(" ", "-").replace("of-", "")
        sov_path = os.path.join(output_dir, f"{slug}-sov.csv")
        with open(sov_path) as f:
            sov_csv = f.read()
        onboard_payload["members"].append({
            "member_name": m["member_name"],
            "sov_csv": sov_csv,
        })
    with open(os.path.join(output_dir, "onboard.json"), "w") as f:
        json.dump(onboard_payload, f, indent=2)


def write_xlsx(pool_data: dict, output_dir: str):
    """Write pool data as Excel files matching the source format."""
    try:
        import openpyxl
    except ImportError:
        print("openpyxl required for xlsx output. Install with: pip install openpyxl")
        print("Falling back to CSV output.")
        return write_csv(pool_data, output_dir)

    os.makedirs(output_dir, exist_ok=True)

    # Summary as JSON
    with open(os.path.join(output_dir, "summary.json"), "w") as f:
        json.dump(pool_data["summary"], f, indent=2)

    sheet_map = {
        "members": ("1_Members.xlsx", "Member"),
        "campuses": ("2_Campuses.xlsx", "Campus"),
        "buildings": ("3_Buildings.xlsx", "Building"),
        "building_valuations": ("3a_Building_Valuations.xlsx", "Valuation"),
        "vehicles": ("5_Licensed_Vehicles.xlsx", "Licensed Vehicle"),
        "vehicle_valuations": ("5a_Vehicle_Valuations.xlsx", "Valuation"),
        "equipment": ("6_Movable_Equipment.xlsx", "Movable Equipment"),
        "equipment_valuations": ("6a_Equipment_Valuations.xlsx", "Valuation"),
        "pito": ("4_Property_in_the_Open.xlsx", "Property in the Open"),
        "pito_valuations": ("4a_PITO_Valuations.xlsx", "Valuation"),
    }

    for key, (filename, sheet_name) in sheet_map.items():
        records = pool_data[key]
        if not records:
            continue
        wb = openpyxl.Workbook()
        ws = wb.active
        ws.title = sheet_name
        headers = list(records[0].keys())
        ws.append(headers)
        for rec in records:
            ws.append([rec[h] for h in headers])
        wb.save(os.path.join(output_dir, filename))


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="Generate simulated CentuRisk risk pool datasets",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument("--pools", type=int, default=1, help="Number of pools to generate (default: 1)")
    parser.add_argument("--size", choices=SIZE_PRESETS.keys(), default="medium", help="Pool size preset (default: medium)")
    parser.add_argument("--members", type=int, default=None, help="Override number of members")
    parser.add_argument("--seed", type=int, default=None, help="Random seed for reproducibility")
    parser.add_argument("--output-dir", type=str, default=None,
                        help="Output directory (default: ../../samples for 'samples' format, ./generated_pools otherwise)")
    parser.add_argument("--format", choices=["samples", "json", "csv", "xlsx"], default="samples",
                        help="Output format (default: samples — produces pool.csv + member-sov.csv for the app)")

    args = parser.parse_args()

    writers = {"samples": write_samples, "json": write_json, "csv": write_csv, "xlsx": write_xlsx}
    writer = writers[args.format]

    # Default output directory: ./samples relative to the project root for 'samples' format
    if args.output_dir is None:
        if args.format == "samples":
            args.output_dir = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", "samples"))
        else:
            args.output_dir = "./generated_pools"

    for i in range(args.pools):
        pool_seed = (args.seed + i) if args.seed is not None else None
        config = make_config(size=args.size, num_members=args.members, seed=pool_seed)

        print(f"\nGenerating pool {i+1}/{args.pools}: {config.pool_name}")
        print(f"  State: {config.state} | Members: {config.num_members} | Seed: {pool_seed}")

        pool_data = generate_pool(config)
        summary = pool_data["summary"]

        # For 'samples' format, use a slug as directory name (what the server expects)
        if args.format == "samples":
            slug = summary["pool_name"].lower().replace(" ", "-")
            pool_dir = os.path.join(args.output_dir, slug)
        else:
            pool_dir = os.path.join(args.output_dir, f"pool_{summary['pool_id']}")

        writer(pool_data, pool_dir)

        print(f"  Campuses:   {summary['campuses']:>6,}")
        print(f"  Buildings:  {summary['buildings']:>6,}")
        print(f"  Vehicles:   {summary['vehicles']:>6,}")
        print(f"  Equipment:  {summary['equipment']:>6,}")
        print(f"  PITO:       {summary['pito']:>6,}")
        print(f"  Total:      {summary['total_assets']:>6,} assets")
        print(f"  Total TIV:  ${summary['total_insurable_value']:>15,.2f}")
        print(f"  Output:     {pool_dir}/")

    print(f"\nDone. Generated {args.pools} pool(s) in {args.output_dir}/")


if __name__ == "__main__":
    main()
