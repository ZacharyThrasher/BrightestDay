"""Trainer items: ball variants (Trap throwables), bike (Piloted), Rare Candy (Grow), Poke Flute (Repel)."""
from plib import S, icon, ALONG_Z, ALONG_X, TIP_NZ, TIP_PZ, TIP_NX, TIP_PX, WHEEL, SEAT, OTHER
from plib import BLUE, RED, YELLOW, PURPLE, PINK, BLACK, CYAN, WHITE, GOLD

SRC = "Pokemon (Game Freak / Nintendo)"


def capture_ball(top='A', extra=None):
    """Capture ball, 8 units: accent top shell, corps-colour lower half (stacked slices), dark band, button."""
    s = S()
    s.ell((0, 4, 0), (8, 8, 8), top)                                      # whole sphere in the top colour
    for y in range(0, 4):                                                 # lower hemisphere re-tinted slice by slice
        r = max(0.6, (16 - (y - 4) ** 2) ** 0.5)
        s.cyl((0, y, 0), (2 * r + 0.1, 1, 2 * r + 0.1), 'P')
    s.cyl((0, 4, 0), (8.4, 1.0, 8.4), 'D')                                # band
    s.cyl((0, 4, -4.0), (3.4, 1.0, 3.4), 'D', ALONG_Z)                    # button ring
    s.cyl((0, 4, -4.4), (1.8, 0.8, 1.8), 'C', ALONG_Z)                    # button
    if extra:
        extra(s)
    return s


def great_marks(s):
    for sx in (-1, 1):                                                    # the two red "wings" on the blue top
        s.box((sx * 2.6, 6.6, -1.0), (1.0, 0.8, 3.2), 'C', (0, sx * 20, sx * 30))


def ultra_marks(s):
    for sx in (-1, 1):                                                    # yellow H stripes on the black top
        s.box((sx * 2.3, 6.6, 0), (0.8, 1.6, 6.0), 'C', (0, 0, sx * 28))
    s.box((0, 7.9, 0), (3.2, 0.6, 1.4), 'C')


def master_marks(s):
    for sx in (-1, 1):                                                    # pink bumps
        s.ell((sx * 3.1, 6.2, -0.4), (1.8, 1.8, 1.8), 'C')
    s.box((-0.9, 6.9, -2.9), (0.6, 1.8, 0.6), 'D', (15, 0, 0))            # the "M"
    s.box((0.9, 6.9, -2.9), (0.6, 1.8, 0.6), 'D', (15, 0, 0))
    s.box((-0.35, 7.1, -2.95), (0.5, 1.2, 0.5), 'D', (15, 0, 25))
    s.box((0.35, 7.1, -2.95), (0.5, 1.2, 0.5), 'D', (15, 0, -25))


def bicycle():
    """~16 units long (in-game 2 m = 16 voxels): chunky tubes so the frame survives voxelising."""
    s = S()
    for pid, z in ((1, -5), (2, 5)):                                      # wheels spin with speed
        s.part(pid, 0, (0, 3.5, z), WHEEL)
        s.cyl((0, 3.5, z), (7, 1.2, 7), 'D', ALONG_X, part=pid)
        s.cyl((0, 3.5, z), (4.6, 1.6, 4.6), 'D', ALONG_X, part=pid, sub=True)
        s.cyl((0, 3.5, z), (1.6, 1.2, 1.6), 'C', ALONG_X, part=pid)        # hub
        s.box((0, 3.5, z), (0.8, 5, 0.8), 'A', part=pid)                  # spokes
        s.box((0, 3.5, z), (0.8, 0.8, 5), 'A', part=pid)
    s.seg((0, 3.5, 5), (0, 4, 0), 1.2, 1.2, 'P')                          # chain stay
    s.seg((0, 3.5, 5), (0, 7.5, 1.2), 1.2, 1.2, 'P')                      # seat stay
    s.seg((0, 4, 0), (0, 7.8, 1.4), 1.3, 1.3, 'P')                        # seat tube
    s.seg((0, 4, 0), (0, 7.6, -3.6), 1.4, 1.4, 'P')                       # down tube
    s.seg((0, 7.2, 1.0), (0, 7.6, -3.6), 1.3, 1.3, 'P')                   # top tube
    s.seg((0, 8.6, -3.8), (0, 3.5, -5), 1.2, 1.2, 'P')                    # fork
    s.box((0, 9.0, -4.0), (6, 1, 1), 'D')                                 # handlebar
    s.box((0, 8.4, -5.4), (2.6, 2.0, 1.8), 'A')                           # front basket
    s.part(255, 0, (0, 8, 1.4), SEAT)             # SEAT_PART id (bd_shapes::voxelize)
    s.ell((0, 8.4, 1.6), (2.2, 1.2, 3.2), 'D', part=255)                    # saddle
    s.cyl((0, 4, 0), (2.6, 1.0, 2.6), 'A', ALONG_X)                       # chainring
    return s


def rare_candy():
    s = S()
    s.ell((0, 3, 0), (5.5, 4.5, 4.5), 'A', (0, 0, 0))                     # candy
    s.ell((0, 3, 0), (5.7, 1.4, 4.7), 'C')                                # white band
    s.box((0, 3.9, -2.1), (1.2, 1.2, 0.5), 'P')                           # "+" sign
    s.box((0, 3.9, -2.1), (2.6, 0.5, 0.5), 'P')
    for sx in (-1, 1):                                                    # twisted wrapper ends
        s.cone((sx * 4.2, 3, 0), (3.4, 2.6, 3.4), 'A', TIP_NX if sx > 0 else TIP_PX)
        s.box((sx * 2.9, 3, 0), (0.6, 1.6, 1.6), 'C')
    return s


def poke_flute():
    s = S()
    s.cyl((0, 0, -4), (1.8, 11, 1.8), 'A', ALONG_Z)                       # body (grip at 0, points -Z)
    s.cone((0, 0, -10.5), (3.2, 2.4, 3.2), 'A', TIP_PZ)                   # bell
    s.cyl((0, 0, -11.3), (2.2, 0.6, 2.2), 'D', ALONG_Z, sub=True)
    s.cyl((0, 0, 1.7), (1.4, 1.2, 1.4), 'D', ALONG_Z)                     # mouthpiece
    for z in (-2, -4, -6, -8):
        s.box((0, 0.9, z), (0.6, 0.6, 0.6), 'D')                          # finger holes
    s.cyl((0, 0, -9.2), (2.2, 0.6, 2.2), 'C', ALONG_Z)                    # ring
    return s


def register():
    icon(["great ball", "greatball", "great pokeball", "great poke ball", "blue pokeball"],
         "Great Ball", "Ball", ["Trap", "Fast"], 0.6, 0.5, accent=BLUE, shape=capture_ball('A', great_marks))
    icon(["ultra ball", "ultraball", "ultra pokeball", "ultra poke ball"],
         "Ultra Ball", "Ball", ["Trap", "Fast", "Heavy"], 0.6, 0.6, accent=BLACK, shape=capture_ball('A', ultra_marks))
    icon(["master ball", "masterball", "master pokeball", "master poke ball", "purple pokeball"],
         "Master Ball", "Ball", ["Trap", "Homing"], 0.6, 0.6, accent=PURPLE, shape=capture_ball('A', master_marks))
    icon(["pokemon bike", "bicycle pokemon", "acro bike", "mach bike", "bike from pokemon", "trainer bike", "misty's bike"],
         "Trainer Bike", "Vehicle", ["Piloted", "Fast"], 2.0, 15, accent=RED, features=["Wheels"], shape=bicycle())
    icon(["rare candy", "rarecandy", "rare candies", "rare candy pokemon"],
         "Rare Candy", "Ball", ["Grow"], 0.8, 0.2, accent=BLUE, shape=rare_candy())
    icon(["poke flute", "pokeflute", "poké flute", "pokemon flute", "poke flute song"],
         "Poke Flute", "Staff", ["Repel"], 1.0, 0.5, accent=CYAN, shape=poke_flute())
