-- Add hash_icons table to support database-driven hash icon management
-- This migration adds support for storing hash icons in the database instead of hardcoded enums

-- Create hash_icons table
CREATE TABLE hash_icons (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    game_id INTEGER REFERENCES games(id) ON DELETE CASCADE,
    file_name VARCHAR(255) NOT NULL,
    racetime_emoji VARCHAR(100) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(game_id, name)
);

ALTER TABLE public.hash_icons OWNER TO mido;

-- Insert initial hash icons for OOTR (game_id = 1)
INSERT INTO hash_icons (name, game_id, file_name, racetime_emoji) VALUES
('Deku Stick', 1, 'HashDekuStick.png', 'HashDekuStick'),
('Deku Nut', 1, 'HashDekuNut.png', 'HashDekuNut'),
('Bow', 1, 'HashBow.png', 'HashBow'),
('Slingshot', 1, 'HashSlingshot.png', 'HashSlingshot'),
('Fairy Ocarina', 1, 'HashFairyOcarina.png', 'HashFairyOcarina'),
('Bombchu', 1, 'HashBombchu.png', 'HashBombchu'),
('Longshot', 1, 'HashLongshot.png', 'HashLongshot'),
('Boomerang', 1, 'HashBoomerang.png', 'HashBoomerang'),
('Lens of Truth', 1, 'HashLensOfTruth.png', 'HashLensOfTruth'),
('Beans', 1, 'HashBeans.png', 'HashBeans'),
('Megaton Hammer', 1, 'HashMegatonHammer.png', 'HashMegatonHammer'),
('Bottled Fish', 1, 'HashBottledFish.png', 'HashBottledFish'),
('Bottled Milk', 1, 'HashBottledMilk.png', 'HashBottledMilk'),
('Mask of Truth', 1, 'HashMaskOfTruth.png', 'HashMaskOfTruth'),
('SOLD OUT', 1, 'HashSoldOut.png', 'HashSoldOut'),
('Cucco', 1, 'HashCucco.png', 'HashCucco'),
('Mushroom', 1, 'HashMushroom.png', 'HashMushroom'),
('Saw', 1, 'HashSaw.png', 'HashSaw'),
('Frog', 1, 'HashFrog.png', 'HashFrog'),
('Master Sword', 1, 'HashMasterSword.png', 'HashMasterSword'),
('Mirror Shield', 1, 'HashMirrorShield.png', 'HashMirrorShield'),
('Kokiri Tunic', 1, 'HashKokiriTunic.png', 'HashKokiriTunic'),
('Hover Boots', 1, 'HashHoverBoots.png', 'HashHoverBoots'),
('Silver Gauntlets', 1, 'HashSilverGauntlets.png', 'HashSilverGauntlets'),
('Gold Scale', 1, 'HashGoldScale.png', 'HashGoldScale'),
('Stone of Agony', 1, 'HashStoneOfAgony.png', 'HashStoneOfAgony'),
('Skull Token', 1, 'HashSkullToken.png', 'HashSkullToken'),
('Heart Container', 1, 'HashHeartContainer.png', 'HashHeartContainer'),
('Boss Key', 1, 'HashBossKey.png', 'HashBossKey'),
('Compass', 1, 'HashCompass.png', 'HashCompass'),
('Map', 1, 'HashMap.png', 'HashMap'),
('Big Magic', 1, 'HashBigMagic.png', 'HashBigMagic');

-- Insert initial hash icons for ALttPR (game_id = 2)
INSERT INTO hash_icons (name, game_id, file_name, racetime_emoji) VALUES
('Bomb', 2, 'HashBombs.png', 'HashBombs'),
('Bombos', 2, 'HashBombos.png', 'HashBombos'),
('Boomerang', 2, 'HashBoomerang.png', 'HashBoomerang'),
('Bow', 2, 'HashBow.png', 'HashBow'),
('Hookshot', 2, 'HashHookshot.png', 'HashHookshot'),
('Mushroom', 2, 'HashMushroom.png', 'HashMushroom'),
('Pendant', 2, 'HashPendant.png', 'HashPendant'),
('Powder', 2, 'HashMagicPowder.png', 'HashMagicPowder'),
('Rod', 2, 'HashIceRod.png', 'HashIceRod'),
('Ether', 2, 'HashEther.png', 'HashEther'),
('Quake', 2, 'HashQuake.png', 'HashQuake'),
('Lamp', 2, 'HashLamp.png', 'HashLamp'),
('Hammer', 2, 'HashHammer.png', 'HashHammer'),
('Shovel', 2, 'HashShovel.png', 'HashShovel'),
('Ocarina', 2, 'HashFlute.png', 'HashFlute'),
('BugNet', 2, 'HashBugnet.png', 'HashBugnet'),
('Book', 2, 'HashBook.png', 'HashBook'),
('Bottle', 2, 'HashEmptyBottle.png', 'HashEmptyBottle'),
('Potion', 2, 'HashGreenPotion.png', 'HashGreenPotion'),
('Cane', 2, 'HashSomaria.png', 'HashSomaria'),
('Cape', 2, 'HashCape.png', 'HashCape'),
('Mirror', 2, 'HashMirror.png', 'HashMirror'),
('Boots', 2, 'HashBoots.png', 'HashBoots'),
('Gloves', 2, 'HashGloves.png', 'HashGloves'),
('Flippers', 2, 'HashFlippers.png', 'HashFlippers'),
('Pearl', 2, 'HashMoonPearl.png', 'HashMoonPearl'),
('Shield', 2, 'HashShield.png', 'HashShield'),
('Tunic', 2, 'HashTunic.png', 'HashTunic'),
('Heart', 2, 'HashHeart.png', 'HashHeart'),
('Map', 2, 'HashMap.png', 'HashMap'),
('Compass', 2, 'HashCompass.png', 'HashCompass'),
('Key', 2, 'HashBigKey.png', 'HashBigKey');

