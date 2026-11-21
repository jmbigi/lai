import sys
import re

class Chess:
    def __init__(self):
        self.board = [
            ['♜','♞','♝','♛','♚','♝','♞','♜'],  # fila 0: negras (rank 8)
            ['♟']*8,                              # fila 1: peones negros
            [' ']*8, [' ']*8, [' ']*8, [' ']*8,  # filas 2-5 vacías
            ['♙']*8,                              # fila 6: peones blancos
            ['♖','♘','♗','♕','♔','♗','♘','♖']   # fila 7: blancas (rank 1)
        ]
        self.turn = 'white'
        self.castle_rights = {'white':{'k':True,'q':True}, 'black':{'k':True,'q':True}}
        self.en_passant = None
        self.move_count = 0

    def print_board(self):
        for i in range(8):
            print(f"{8-i} {' '.join(self.board[i])} {8-i}")
        print("  a b c d e f g h")

    def parse_coord(self, s):
        c, f = s[0], s[1]
        return (8 - int(f), ord(c) - 97)

    def is_valid_move(self, start, end):
        start_row, start_col = start
        end_row, end_col = end
        piece = self.board[start_row][start_col]
        color = self.turn

        if piece == ' ':
            return False

        if piece.islower():
            color = 'black'

        if piece == '♟':
            return self.is_valid_pawn_move(start, end, color)
        elif piece == '♙':
            return self.is_valid_pawn_move(start, end, color)
        elif piece == '♜' or piece == '♖':
            return self.is_valid_rook_move(start, end)
        elif piece == '♞' or piece == '♘':
            return self.is_valid_knight_move(start, end)
        elif piece == '♝' or piece == '♗':
            return self.is_valid_bishop_move(start, end)
        elif piece == '♛' or piece == '♕':
            return self.is_valid_queen_move(start, end)
        elif piece == '♚' or piece == '♔':
            return self.is_valid_king_move(start, end)

        return False

    def is_valid_pawn_move(self, start, end, color):
        start_row, start_col = start
        end_row, end_col = end
        piece = self.board[start_row][start_col]
        direction = -1 if color == 'white' else 1
        is_first_move = (start_row == 6 and color == 'white') or (start_row == 1 and color == 'black')

        if end_col == start_col:
            if (end_row == start_row + direction) or (is_first_move and end_row == start_row + 2 * direction):
                if self.board[end_row][end_col] == ' ':
                    return True
                return False
            return False
        elif abs(end_col - start_col) == 1 and end_row == start_row + direction:
            if self.board[end_row][end_col] != ' ':
                return True
            return False
        return False

    def is_valid_rook_move(self, start, end):
        start_row, start_col = start
        end_row, end_col = end
        if start_row == end_row:
            for col in range(min(start_col, end_col) + 1, max(start_col, end_col)):
                if self.board[start_row][col] != ' ':
                    return False
            return True
        elif start_col == end_col:
            for row in range(min(start_row, end_row) + 1, max(start_row, end_row)):
                if self.board[row][start_col] != ' ':
                    return False
            return True
        return False

    def is_valid_knight_move(self, start, end):
        start_row, start_col = start
        end_row, end_col = end
        dr = abs(end_row - start_row)
        dc = abs(end_col - start_col)
        return (dr == 2 and dc == 1) or (dr == 1 and dc == 2)

    def is_valid_bishop_move(self, start, end):
        start_row, start_col = start
        end_row, end_col = end
        if abs(end_row - start_row) != abs(end_col - start_col):
            return False
        step_row = 1 if end_row > start_row else -1
        step_col = 1 if end_col > start_col else -1
        for i in range(1, abs(end_row - start_row)):
            r = start_row + i * step_row
            c = start_col + i * step_col
            if self.board[r][c] != ' ':
                return False
        return True

    def is_valid_queen_move(self, start, end):
        return self.is_valid_rook_move(start, end) or self.is_valid_bishop_move(start, end)

    def is_valid_king_move(self, start, end):
        start_row, start_col = start
        end_row, end_col = end
        dr = abs(end_row - start_row)
        dc = abs(end_col - start_col)
        return dr <= 1 and dc <= 1

    def is_in_check(self, color):
        king_pos = None
        for i in range(8):
            for j in range(8):
                if self.board[i][j] == ('♔' if color == 'white' else '♚'):
                    king_pos = (i, j)
                    break
            if king_pos:
                break
        if not king_pos:
            return False
        for i in range(8):
            for j in range(8):
                piece = self.board[i][j]
                if (piece.islower() and color == 'white') or (piece.isupper() and color == 'black'):
                    if self.is_attacking(i, j, king_pos):
                        return True
        return False

    def is_attacking(self, row, col, king_pos):
        dr = abs(row - king_pos[0])
        dc = abs(col - king_pos[1])
        piece = self.board[row][col]
        if piece == '♜' or piece == '♖':
            return dr == 0 or dc == 0
        elif piece == '♞' or piece == '♘':
            return dr == 2 and dc == 1 or dr == 1 and dc == 2
        elif piece == '♝' or piece == '♗':
            return dr == dc
        elif piece == '♛' or piece == '♕':
            return dr == 0 or dc == 0 or dr == dc
        elif piece == '♟' or piece == '♙':
            return (dr == 1 and dc == 1) or (dr == 1 and dc == 0)
        return False

    def is_checkmate(self, color):
        if not self.is_in_check(color):
            return False
        for i in range(8):
            for j in range(8):
                if self.board[i][j] == ('♙' if color == 'white' else '♟'):
                    for di in [-1, 0, 1]:
                        for dj in [-1, 0, 1]:
                            if di == 0 and dj == 0:
                                continue
                            ni, nj = i + di, j + dj
                            if 0 <= ni < 8 and 0 <= nj < 8:
                                if self.is_valid_move((i, j), (ni, nj)):
                                    return False
        return True

    def is_stalemate(self):
        if self.is_in_check(self.turn):
            return False
        for i in range(8):
            for j in range(8):
                if self.board[i][j] == ('♙' if self.turn == 'white' else '♟'):
                    for di in [-1, 0, 1]:
                        for dj in [-1, 0, 1]:
                            if di == 0 and dj == 0:
                                continue
                            ni, nj = i + di, j + dj
                            if 0 <= ni < 8 and 0 <= nj < 8:
                                if self.is_valid_move((i, j), (ni, nj)):
                                    return False
        return True

    def move_piece(self, start, end):
        start_row, start_col = start
        end_row, end_col = end
        piece = self.board[start_row][start_col]
        color = self.turn

        if piece == ' ':
            return False

        if piece.islower():
            color = 'black'

        if not self.is_valid_move(start, end):
            return False

        self.board[end_row][end_col] = piece
        self.board[start_row][start_col] = ' '

        if piece == '♟' or piece == '♙':
            if end_row == 0 or end_row == 7:
                self.board[end_row][end_col] = '♕' if color == 'white' else '♛'

        if self.turn == 'white':
            self.turn = 'black'
        else:
            self.turn = 'white'

        self.move_count += 1

        return True

    def check_game_state(self):
        if self.is_checkmate(self.turn):
            print(f"{self.turn} is in checkmate!")
            return True
        if self.is_stalemate():
            print("Stalemate!")
            return True
        return False

    def run(self):
        while True:
            self.print_board()
            input_str = input(f"{self.turn} > ").strip()
            if input_str == "quit":
                break
            moves = input_str.split()
            if len(moves) != 2:
                continue
            start = self.parse_coord(moves[0])
            end = self.parse_coord(moves[1])
            if self.move_piece(start, end):
                if self.check_game_state():
                    break