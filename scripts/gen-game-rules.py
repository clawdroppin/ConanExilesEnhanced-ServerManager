"""Generate src/data/gameRules.ts from docs/research/enhanced-settings-facts.json.

Run from the repo root:  python scripts/gen-game-rules.py
The facts file holds what was verified against a real Enhanced 3.0.1 server (keys, defaults a fresh
server writes, restart flags, dependencies). Labels, groups and descriptions live below – edit them
here, not in the generated file.
"""
import json, re, pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
facts = json.loads((ROOT / 'docs/research/enhanced-settings-facts.json').read_text(encoding='utf-8'))
doc, fd = facts['doc'], facts['fileDefaults']

# Never shown in Game Rules: profile-owned (Server Settings / Mods), server-owned, or doc templates.
SKIP = {'AdminPassword', 'ServerPassword', 'ServerMessageOfTheDay', 'ServerModList', 'ConfigVersion',
        'BlueprintConfigVersion', 'CachedBuildingReplicationDistance', 'CachedLandClaimRadiusMultiplier',
        'ServerVoiceChat'}  # the file key is serverVoiceChat

# Enum index order verified from the shipping binary (also in core/settings.rs ENUMS).
PRIVACY = [('0', 'Everyone can see'), ('1', 'Owners can see'), ('2', 'Admins only'), ('3', 'Nobody')]
ENUMS = {
    'DropEquipmentOnDeath': [('0', 'Drop nothing'), ('1', 'Drop everything'), ('2', 'Drop backpack only')],
    'MaxNudity': [('0', 'None'), ('1', 'Partial'), ('2', 'Full')],
    'ShowOnlinePlayers': [('0', 'Show everyone'), ('1', 'Clan members only'), ('2', 'Nobody')],
    'EventLogPvECauserPrivacy': PRIVACY,
    'EventLogPvPCauserPrivacy': PRIVACY,
    'serverVoiceChat': [('0', 'Disabled'), ('1', 'Enabled')],
    'CombatModeModifier': [('0', 'None (classic)'), ('1', 'Fast'), ('2', 'Mayhem')],
    'BuildingReplicationDistance': [('0', 'Normal'), ('1', 'Far'), ('2', 'Cinematic')],
    'CreativeModeServer': [('0', 'Admins only'), ('1', 'Everyone may toggle'), ('2', 'Forced on for everyone')],
    'serverRegion': [('0', 'Europe'), ('1', 'North America'), ('2', 'Asia'), ('3', 'Oceania'), ('4', 'South America'), ('5', 'Japan'), ('6', 'All regions')],
    'NPCMindReadingMode': [('0', 'Off'), ('1', 'Mode 1'), ('2', 'Mode 2')],
    'LQAVPMethod': [('0', 'Method 0'), ('1', 'Method 1'), ('2', 'Method 2 (default)')],
}
ENUM_DOC_DEFAULT = {'DropEquipmentOnDeath': '1', 'ShowOnlinePlayers': '0', 'serverVoiceChat': '1'}

TEXT = {'BuildingPVPWhitelist', 'ItemRepairDurabilityLossByRepairkitTier', 'RegionBlockList', 'ServerTransferServersWhitelist',
        'ServerMergeDestination', 'FeatsBlacklist', 'ServerBlacklistedFeatIDs', 'IPBlackList', 'ServerBuildingPVPWhitelistIDs'}

DAYS = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday']

GROUPS = [
    ('general', 'General & access'),
    ('pvp', 'PvP & raiding'),
    ('schedule', 'PvP schedule'),
    ('death', 'Death & loot'),
    ('combat', 'Combat & damage'),
    ('survival', 'Survival & movement'),
    ('stamina', 'Stamina'),
    ('xp', 'Experience'),
    ('harvest', 'Harvesting, crafting & items'),
    ('building', 'Building & land claim'),
    ('decay', 'Decay & abandonment'),
    ('thralls', 'Thralls, pets & followers'),
    ('npcs', 'NPCs & creatures'),
    ('storm', 'Storm, surge & Elder Things'),
    ('avatars', 'Avatars'),
    ('time', 'Day & night'),
    ('chat', 'Chat, clans & social'),
    ('regions', 'Server browser & regions'),
    ('transfer', 'Transfers & merges'),
    ('tech', 'Technical & anti-cheat'),
]

def group_of(k):
    kl = k.lower()
    rules = [
        ('schedule', r'^pvp(time|enabled(monday|tuesday|wednesday|thursday|friday|saturday|sunday))|^pvpbuildingdamage(time|enabled)'),
        ('pvp', r'pvp|candamageplayerowned|dynamicbuildingdamage|combatmode|friendlyfire|restrictpvp|disablebuildingduringtime'),
        ('death', r'dropequipment|corpse|lootcorpse|unconscious|deathmap|logoutcharacters|offlineplayersunconscious'),
        ('avatars', r'avatar'),
        ('storm', r'storm|elderthing|surge|sandstorm'),
        ('thralls', r'thrall|follower|minion|animalpen|feedbox|tether|rescue'),
        ('stamina', r'stamina'),
        ('xp', r'xp'),
        ('decay', r'decay|abandon|autodemolish'),
        ('building', r'build|landclaim|structure|stability|campsignore|poiprotection|containersignore|noownership|undermesh|allowedtimeundermesh'),
        ('npcs', r'npc|ambient|dogsofthedesert|aggro|mindreading|purge'),
        ('harvest', r'harvest|craft|resource|itemspoil|itemconver|durability|repair|fuel|spoil|altar|vault|shield'),
        ('time', r'daycycle|daytime|nighttime|dawndusk'),
        ('chat', r'chat|clan|showonline|voicechat|eventlog|healthbar|fatalit|targetlock|eventsystem'),
        ('regions', r'region'),
        ('transfer', r'transfer|merge|retirement|canimport'),
        ('survival', r'hunger|thirst|water|health|movement|sprint|encumbrance|knockback|corruption|conscious|nudity|damage'),
        ('combat', r'damage|knockback'),
        ('general', r'creative|battleye|whitelist|kickafk|loginqueue|disconnection|familyshared|maxallowedping|bancheck|nudity|building'),
    ]
    for g, pat in rules:
        if re.search(pat, kl):
            return g
    return 'tech'

LABELS = {
    'PVPEnabled': 'PvP enabled', 'RestrictPVPTime': 'Restrict PvP to a schedule', 'RestrictPVPBuildingDamageTime': 'Restrict building damage to a schedule',
    'CanDamagePlayerOwnedStructures': 'Players can damage buildings', 'DynamicBuildingDamage': 'Dynamic building damage', 'DynamicBuildingDamagePeriod': 'Dynamic building damage period',
    'DisableBuildingDuringTimeRestrictedPVP': 'No building while PvP is on', 'FriendlyFireDamageMultiplier': 'Friendly fire damage', 'CombatModeModifier': 'Combat mode',
    'DropEquipmentOnDeath': 'Drop items on death', 'EverybodyCanLootCorpse': 'Everybody can loot corpses', 'LogoutCharactersRemainInTheWorld': 'Logged-out characters stay in the world',
    'OfflinePlayersUnconsciousBodiesHours': 'Offline bodies remain for', 'UnconsciousTimeSeconds': 'Unconscious time', 'CorpsesPerPlayer': 'Corpses kept per player',
    'MaxDeathMapMarkers': 'Death markers on the map', 'MaxNudity': 'Maximum nudity', 'CreativeModeServer': 'Creative mode', 'IsBattlEyeEnabled': 'BattlEye anticheat',
    'EnableWhitelist': 'Whitelist only', 'KickAFKTime': 'Kick AFK players after', 'KickAFKPercentage': 'AFK kick when server is at least',
    'EnableLoginQueue': 'Login queue', 'DisconnectionGraceTime': 'Reconnect grace time', 'AllowFamilySharedAccount': 'Allow Steam Family Sharing',
    'MaxAllowedPing': 'Maximum ping', 'serverRegion': 'Server region', 'serverVoiceChat': 'Voice chat', 'ShowOnlinePlayers': 'Show online players',
    'ClanMaxSize': 'Maximum clan size', 'ChatHasGlobal': 'Global chat', 'ChatLocalRadius': 'Local chat radius', 'ChatMaxMessageLength': 'Max chat message length',
    'HarvestAmountMultiplier': 'Harvest amount', 'ResourceRespawnSpeedMultiplier': 'Resource respawn speed', 'CraftingCostMultiplier': 'Crafting cost',
    'ItemConvertionMultiplier': 'Crafting speed (item conversion time)', 'FuelBurnTimeMultiplier': 'Fuel burn time', 'ItemSpoilRateScale': 'Food spoil rate', 'DurabilityMultiplier': 'Durability loss',
    'LandClaimRadiusMultiplier': 'Land claim radius', 'BuildingDamageMultiplier': 'Building damage', 'NoOwnership': 'No ownership (anyone can use anything)',
    'ContainersIgnoreOwnership': 'Containers ignore ownership', 'AllowBuildingAnywhere': 'Allow building anywhere', 'DisableBuildingAbandonment': 'Disable building decay',
    'BuildingDecayTimeMultiplier': 'Building decay time', 'DisableThrallDecay': 'Disable thrall decay', 'ThrallDecayTime': 'Thrall decay time',
    'ThrallConversionMultiplier': 'Thrall conversion time', 'AvatarsDisabled': 'Disable avatars', 'AvatarLifetime': 'Avatar lifetime', 'AvatarSummonTime': 'Avatar summon time',
    'RestrictAvatarSummoningTime': 'Restrict avatar summoning to a schedule', 'StormEnabled': 'Sandstorms', 'EnableSandStorm': 'Sandstorm (legacy switch)',
    'ElderThingsEnabled': 'Elder Things during storms', 'SiegeElderThingsEnabled': 'Siege Elder Things', 'PlayerXPRateMultiplier': 'XP (all sources)',
    'PlayerXPKillMultiplier': 'XP from kills', 'PlayerXPHarvestMultiplier': 'XP from harvesting', 'PlayerXPCraftMultiplier': 'XP from crafting', 'PlayerXPTimeMultiplier': 'XP over time',
    'DayCycleSpeedScale': 'Day cycle speed', 'DayTimeSpeedScale': 'Day time speed', 'NightTimeSpeedScale': 'Night time speed', 'DawnDuskSpeedScale': 'Dawn & dusk speed',
    'NPCRespawnMultiplier': 'NPC respawn time', 'NPCHealthMultiplier': 'NPC health', 'NPCDamageMultiplier': 'NPC damage dealt', 'NPCDamageTakenMultiplier': 'NPC damage taken',
    'PlayerDamageMultiplier': 'Player damage dealt', 'PlayerDamageTakenMultiplier': 'Player damage taken', 'MinionDamageMultiplier': 'Thrall damage dealt', 'MinionDamageTakenMultiplier': 'Thrall damage taken',
    'StructureDamageMultiplier': 'Structure damage', 'StructureHealthMultiplier': 'Structure health', 'PlayerHealthRegenSpeedScale': 'Health regeneration speed',
    'PlayerMovementSpeedScale': 'Movement speed', 'PlayerSprintSpeedScale': 'Sprint speed', 'PlayerEncumbranceMultiplier': 'Carry weight', 'PlayerEncumbrancePenaltyMultiplier': 'Encumbrance penalty',
    'PlayerActiveHungerMultiplier': 'Hunger (active)', 'PlayerActiveThirstMultiplier': 'Thirst (active)', 'PlayerIdleHungerMultiplier': 'Hunger (idle)', 'PlayerIdleThirstMultiplier': 'Thirst (idle)',
    'PlayerOfflineHungerMultiplier': 'Hunger (offline)', 'PlayerOfflineThirstMultiplier': 'Thirst (offline)', 'PlayerCorruptionGainMultiplier': 'Corruption gain',
    'PlayerCorruptionGainFromSorceryMultiplier': 'Corruption gain from sorcery', 'ThrallCorruptionRemovalMultiplier': 'Thrall corruption removal', 'MaxAggroRange': 'NPC max aggro range',
    'PlayerStaminaCostMultiplier': 'Stamina cost', 'PlayerStaminaCostSprintMultiplier': 'Stamina cost (sprint)', 'PlayerStaminaRegenSpeedScale': 'Stamina regeneration speed',
    'StaminaCostMultiplier': 'Stamina cost (global)', 'UseMinionPopulationLimit': 'Limit thrall population', 'BuildingReplicationDistance': 'Building view distance',
    'EnableTargetLock': 'Target lock', 'EnableFatalities': 'Fatalities', 'EnableClanMarkers': 'Clan map markers', 'HealthbarVisibilityDistance': 'Health bar distance',
    'CraftFromStorageRadius': 'Craft-from-storage radius', 'BuildFromStorageRadius': 'Build-from-storage radius', 'PVPBuildFromStorageRadius': 'Build-from-storage radius (PvP)',
    'PersonalCraftFromStorageRadius': 'Personal craft-from-storage radius', 'BuildingPickupEnabled': 'Pick up buildings', 'ServerTransferEnabled': 'Character transfers',
    'StabilityLossMultiplier': 'Stability loss', 'EnableFollowerDbno': 'Followers get downed instead of dying', 'NPCMaxSpawnCapMultiplier': 'NPC spawn cap',
    'MaxAmbientElderThings': 'Max ambient Elder Things', 'AmbientElderThingRespawnRate': 'Ambient Elder Thing respawn rate', 'DecoupleSurgeFromStorm': 'Decouple surge from storm',
    'DecoupledSurgeCooldown': 'Decoupled surge cooldown', 'MaxPlayerCorpsesInArea': 'Max player corpses in an area', 'PlayerCorpseLifeTime': 'Player corpse lifetime', 'NPCCorpseLifeTime': 'NPC corpse lifetime',
}

DESC = {
    'DropEquipmentOnDeath': 'What a player drops when they die. “Drop nothing” keeps your gear. Admins can also change this in-game; the manager keeps whatever was set last.',
    'EverybodyCanLootCorpse': 'Anyone can loot a dead player’s body, not just the owner and clan.',
    'PVPEnabled': 'Players can hurt each other. Needed for every other PvP option.',
    'CanDamagePlayerOwnedStructures': 'Lets players raid each other’s buildings (needs PvP).',
    'RestrictPVPTime': 'Only allow PvP during the hours set under PvP schedule.',
    'RestrictPVPBuildingDamageTime': 'Only allow building damage (raiding) during the scheduled hours.',
    'NoOwnership': 'Everyone can open every door, chest and placeable. Needs a restart.',
    'ContainersIgnoreOwnership': 'Chests and crafting stations can be opened by anyone.',
    'DisableBuildingAbandonment': 'When on, buildings never decay. Turn off to enable the decay timers below. Needs a restart.',
    'EnableWhitelist': 'Only players on the whitelist (Players → Whitelist) can join.',
    'IsBattlEyeEnabled': 'BattlEye cheat protection. Some mods/tools need it off. Takes effect on the next start.',
    'CreativeModeServer': 'Who can use creative mode (free building, no costs).',
    'serverRegion': 'Region the server is listed under in the in-game browser.',
    'KickAFKTime': 'Idle players are kicked after this long – only when the server is at least as full as the percentage below.',
    'KickAFKPercentage': 'AFK kicks only happen once the server is this full (percent of max players).',
    'MaxAllowedPing': 'Players above this ping are kicked. 0 = no limit.',
    'HarvestAmountMultiplier': 'How much you get per hit when harvesting.',
    'ItemConvertionMultiplier': 'Crafting time multiplier – lower is faster.',
    'ThrallConversionMultiplier': 'Wheel of Pain conversion time – lower is faster.',
    'ItemSpoilRateScale': 'How fast food and perishables spoil.',
    'DurabilityMultiplier': 'How fast gear loses durability – lower lasts longer.',
    'LandClaimRadiusMultiplier': 'Size of the no-build zone around buildings. Needs a restart.',
    'BuildingReplicationDistance': 'How far away buildings are visible. Further costs bandwidth and CPU. Needs a restart.',
    'UnconsciousTimeSeconds': 'How long a downed (unconscious) player stays down.',
    'LogoutCharactersRemainInTheWorld': 'Characters of logged-out players stay in the world (sleeping) and can be attacked.',
    'StormEnabled': 'Sandstorms sweep across the map.',
    'AvatarsDisabled': 'Turns god avatars off completely.',
    'DisableThrallDecay': 'When on, thralls never decay. Turn off to use the decay time below.',
    'MaxNudity': 'Highest nudity level players can choose.',
    'ShowOnlinePlayers': 'Who sees the list of online players.',
    'UseMinionPopulationLimit': 'Caps the number of thralls and pets per clan/player (performance).',
    'EnableLoginQueue': 'Players joining a full server wait in a queue instead of being refused.',
    'DayCycleSpeedScale': 'Overall speed of the day/night cycle. Lower = longer days.',
    'MaxAmbientElderThings': 'Undocumented but real (read from the binary).',
    'DecoupleSurgeFromStorm': 'Undocumented but real: run surges independently of sandstorms.',
}

SUFFIX_RE = [
    (r'(Lifetime|LifeTime|Time|Cooldown|Timer|Period|Duration|GraceTime|Seconds)$', 's'),
]
SECONDS_KEYS = {'AvatarLifetime', 'AvatarSummonTime', 'BuildingDecayTimePerScore', 'DamageCooldownBeforeRescue', 'DecayBonusTimeRate', 'DisconnectionGraceTime',
                'DynamicBuildingDamagePeriod', 'FollowerRescueCooldown', 'KickAFKTime', 'MaxBuildingDecayTime', 'MaxDecayTimeToAutoDemolish', 'NPCCorpseLifeTime',
                'PlayerCorpseLifeTime', 'ThrallDecayTime', 'UnconsciousTimeSeconds', 'SurgeDespawnTimer', 'MinionOverpopulationCleanup', 'ElderThingsIdleLifespan',
                'StaminaExhaustionTime', 'StaminaRegenerationTime', 'StaminaOnConsumeRegenPause', 'StaminaOnExhaustionRegenPause', 'AllowedTimeUndermesh', 'DecoupledSurgeCooldown'}
MINUTES_KEYS = {'StormCooldown', 'StormDuration', 'ThrallScoutingTimeMinutes', 'ThrallTeleportingCooldown'}
DISTANCE_KEYS = {'BuildFromStorageRadius', 'CraftFromStorageRadius', 'PersonalCraftFromStorageRadius', 'PVPBuildFromStorageRadius', 'ChatLocalRadius', 'HealthbarVisibilityDistance',
                 'MaxAggroRange', 'ThrallExclusionRadius', 'ThrallMinDistanceAwayFromHome', 'CoopTetheringLimit', 'TetheringMaxLimit', 'TetheringMinLimit', 'TetheringRangeDelta'}
UNDOCUMENTED = {'MaxAmbientElderThings': '700', 'AmbientElderThingRespawnRate': '1', 'DecoupleSurgeFromStorm': 'False', 'DecoupledSurgeCooldown': '0', 'MaxPlayerCorpsesInArea': '0'}

def norm_default(k):
    if k in fd:
        return fd[k]
    v = (doc.get(k) or {}).get('docDefault', '')
    if k in ENUM_DOC_DEFAULT:
        return ENUM_DOC_DEFAULT[k]
    if k in UNDOCUMENTED:
        return UNDOCUMENTED[k]
    if v in ('TRUE', 'FALSE'):
        return v.capitalize()
    if v == '-':
        return ''
    m = re.fullmatch(r'([0-9.]+)\.?f?', v)
    if m:
        n = m.group(1).rstrip('.')
        return n[:-2] if n.endswith('.0') else n
    if k == 'UnconsciousTimeSeconds':
        return '600'
    return v

def label_of(k):
    if k in LABELS:
        return LABELS[k]
    m = re.match(r'^PVP(Time|BuildingDamageTime)(\w+?)(Start|End)$', k)
    if m:
        return f"{'PvP' if m.group(1) == 'Time' else 'Building damage'} {m.group(2)} {m.group(3).lower()}"
    m = re.match(r'^PVP(Enabled|BuildingDamageEnabled)(\w+)$', k)
    if m:
        return f"{'PvP' if m.group(1) == 'Enabled' else 'Building damage'} on {m.group(2)}"
    m = re.match(r'^AvatarSummoningTime(Weekday|Weekend)(Start|End)$', k)
    if m:
        return f"Avatar summoning {m.group(1).lower()} {m.group(2).lower()}"
    m = re.match(r'^RegionAllow(\w+)$', k)
    if m:
        return 'Allow players from ' + re.sub(r'(?<!^)(?=[A-Z])', ' ', m.group(1))
    words = re.sub(r'Multiplier$', '', k)
    words = re.sub(r'^b(?=[A-Z])', '', words)
    words = re.sub(r'(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])', ' ', words)
    words = words.replace('P V P', 'PvP').replace('N P C', 'NPC').replace('X P', 'XP').replace('L Q A V P', 'LQAVP').replace('Pv P', 'PvP')
    out = words[0].upper() + words[1:].lower() if words else k
    for a, b in [('pv e', 'PvE'), ('pv p', 'PvP'), (' i ds', ' IDs'), ('np cs', 'NPCs'), ('ip black list', 'IP blacklist'), ('poi ', 'POI ')]:
        out = re.sub(re.escape(a), b, out, flags=re.I)
    for w in ['PvP', 'PvE', 'NPC', 'XP', 'LQAVP', 'Elder', 'Things', 'Thing', 'IDs', 'IP', 'AFK']:
        out = re.sub(r'\b' + w.lower() + r'\b', w, out, flags=re.I)
    return out

def type_of(k, d):
    if k in ENUMS:
        return 'enum'
    if k in TEXT or k.startswith('Storm' + 'Time') or (d.startswith('(') and d.endswith(')')):
        return 'text'
    if re.match(r'^(PVPTime|PVPBuildingDamageTime|AvatarSummoningTime)\w+(Start|End)$', k):
        return 'hhmm'
    if d in ('True', 'False'):
        return 'bool'
    if re.fullmatch(r'-?\d+', d or 'x') and not re.search(r'Multiplier|Scale|Rate', k):
        return 'int'
    if re.fullmatch(r'-?[0-9.]+', d or 'x') or re.search(r'Multiplier|Scale|Rate', k):
        return 'float'
    return 'text'

keys = sorted((set(doc) | set(fd) | set(UNDOCUMENTED)) - SKIP, key=str.lower)
keys = [k for k in keys if '[' not in k]

rows = []
for k in keys:
    d = norm_default(k)
    t = type_of(k, d)
    meta = doc.get(k, {})
    rel = [r for r in meta.get('reliesOn', []) if re.fullmatch(r'!?[A-Za-z]+', r)]
    row = {'key': k, 'label': label_of(k), 'group': group_of(k), 'type': t, 'def': d}
    if k in DESC:
        row['desc'] = DESC[k]
    if t == 'enum':
        row['options'] = [{'value': v, 'label': l} for v, l in ENUMS[k]]
    if t == 'float':
        row['step'] = 0.05 if 'Multiplier' in k or 'Scale' in k else 0.1
        row['min'] = 0
    if t == 'int':
        row['min'] = 0
    if k in SECONDS_KEYS:
        row['suffix'] = 's'
    elif k in MINUTES_KEYS:
        row['suffix'] = 'min'
    elif k in DISTANCE_KEYS:
        row['suffix'] = 'cm'
    elif k == 'OfflinePlayersUnconsciousBodiesHours':
        row['suffix'] = 'h'
    elif k == 'KickAFKPercentage':
        row['suffix'] = '%'
    elif t == 'float' and ('Multiplier' in k or 'Scale' in k):
        row['suffix'] = '×'
    if meta.get('restart'):
        row['restart'] = True
    if rel:
        row['requires'] = rel
    if k in UNDOCUMENTED:
        row['undocumented'] = True
    rows.append(row)

out = ['// GENERATED by scripts/gen-game-rules.py from docs/research/enhanced-settings-facts.json – do not edit by hand.',
       '// Every key of ServerSettings.ini [ServerSettings] the Enhanced 3.0.1 server knows (minus the ones managed',
       '// under Server Settings / Mods). Values are in FILE form: enums are indexes ("0"), bools "True"/"False".',
       '// The backend translates enum indexes to names for live RCON changes (core/settings.rs ENUMS).',
       '',
       "export type RuleType = 'bool' | 'int' | 'float' | 'enum' | 'text' | 'hhmm';",
       '',
       'export interface GameRule {',
       '  key: string;',
       '  label: string;',
       '  group: string;',
       '  type: RuleType;',
       "  /** What the server uses when the key is absent (what a fresh server writes). '' = unknown. */",
       '  def: string;',
       '  desc?: string;',
       '  options?: { value: string; label: string }[];',
       '  min?: number;',
       '  step?: number;',
       '  suffix?: string;',
       '  /** Only takes effect after a restart. */',
       '  restart?: boolean;',
       "  /** Other settings this one depends on (`!X` = needs X off). */",
       '  requires?: string[];',
       '  /** Real setting found in the binary but missing from Funcom’s docs. */',
       '  undocumented?: boolean;',
       '}',
       '',
       'export const RULE_GROUPS: { id: string; label: string }[] = ' + json.dumps([{'id': g, 'label': l} for g, l in GROUPS], ensure_ascii=False) + ';',
       '',
       'export const GAME_RULES: GameRule[] = [']
for r in rows:
    out.append('  ' + json.dumps(r, ensure_ascii=False) + ',')
out.append('];')
out.append('')
(ROOT / 'src/data/gameRules.ts').write_text('\n'.join(out), encoding='utf-8')
print(len(rows), 'rules')
from collections import Counter
print(Counter(r['group'] for r in rows))
