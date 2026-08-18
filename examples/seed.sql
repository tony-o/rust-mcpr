CREATE TABLE books (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    title   TEXT NOT NULL,
    author  TEXT NOT NULL,
    year    INTEGER NOT NULL,
    summary TEXT NOT NULL
);

INSERT INTO books (title, author, year, summary) VALUES
    ('The C Programming Language', 'Kernighan & Ritchie', 1978, 'The book that taught a generation why pointers deserve their reputation.'),
    ('Structure and Interpretation of Computer Programs', 'Abelson & Sussman', 1985, 'MIT''s opening argument that everything is a procedure if you squint hard enough.'),
    ('The Pragmatic Programmer', 'Hunt & Thomas', 1999, 'Career advice disguised as a programming book, or maybe it''s the other way around.'),
    ('Snow Crash', 'Neal Stephenson', 1992, 'A pizza delivery driver, a virtual reality metaverse, and a linguistic virus.'),
    ('The Hitchhiker''s Guide to the Galaxy', 'Douglas Adams', 1979, 'Earth gets demolished for a hyperspace bypass. Bring a towel.'),
    ('Neuromancer', 'William Gibson', 1984, 'Coined ''cyberspace'' and then made it look effortless.');
